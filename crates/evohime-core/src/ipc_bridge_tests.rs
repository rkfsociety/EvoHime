mod tests {
    use super::*;
    use crate::CoreEvent;
    use tokio::io::duplex;

    fn sample_typed_ledger_event(
        event_id: &str,
        action_id: &str,
    ) -> execution_ledger::ExecutionEventV1 {
        execution_ledger::ExecutionEventV1 {
            schema_version: 1,
            event_id: event_id.to_string(),
            sequence_id: None,
            run_scope: execution_ledger::RunScope::Standalone,
            run_id: "run-ipc-1".into(),
            session_id: Some("session-ipc-1".into()),
            task_id: "task-ipc".into(),
            created_at_ms: 1_700_000_000_000,
            state_after: Some(execution_ledger::ActionState::Running),
            action_id: Some(action_id.to_string()),
            tool_call_id: None,
            observation_id: None,
            receipt_id: None,
            failure_id: None,
            workflow_run_id: None,
            node_id: None,
            attempt_id: None,
            effect_id: None,
            model_request_id: None,
            body: execution_ledger::ExecutionEventBody::ToolCall {
                tool_name: "shell".into(),
                tool_call_hash: "hash-1".into(),
                manifest_hash: None,
            },
            redaction: execution_ledger::RedactionMeta::default(),
        }
    }

    /// Typed ledger rows written by 08-2's `append_ledger_event` must reach
    /// the IPC replay path (план 08-3) as an additive `execution_event`
    /// projection, without disturbing the generic backward-compat fields.
    #[tokio::test]
    async fn push_journal_tail_projects_typed_ledger_row_into_execution_event() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-ledger-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let source_event = sample_typed_ledger_event("event-ipc-1", "action-ipc-1");
        {
            let database = journal.database().lock().await;
            database
                .append_ledger_event(&source_event)
                .expect("typed event appends");
        }
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, mut server) = duplex(16 * 1024);

        bridge
            .push_journal_tail(&mut server, 0)
            .await
            .expect("tail pushes");
        let frame = transport::read_frame(&mut client)
            .await
            .expect("frame reads");
        let envelope = generated::EventEnvelope::decode(frame.as_slice()).expect("frame decodes");

        assert_eq!(envelope.event_type, "ledger.tool_call");
        assert!(
            !envelope.payload.is_empty(),
            "generic payload stays populated"
        );
        let projected = match envelope.event {
            Some(generated::event_envelope::Event::ExecutionEvent(projected)) => projected,
            other => panic!("expected ExecutionEvent oneof, got {other:?}"),
        };
        assert_eq!(projected.event_id, "event-ipc-1");
        assert_eq!(projected.action_id, "action-ipc-1");
        assert_eq!(projected.run_scope, "standalone");
        assert_eq!(projected.state_after, "running");
        let body: execution_ledger::ExecutionEventBody =
            serde_json::from_slice(&projected.body_json).expect("body_json decodes");
        assert_eq!(body, source_event.body);
        let _ = std::fs::remove_file(&path);
    }

    /// План 08-4 acceptance: "reconnect во время каждой промежуточной
    /// фазы" — the typed IPC projection is generic over `state_after`, not
    /// special-cased to whatever phase happened to be tested elsewhere.
    /// Replays a run whose last known phase is `waiting_approval` and one
    /// whose last known phase is `cancelling` (the state this plan's own
    /// CHECK-rebuild migration exists to allow), proving both reconnect
    /// correctly rather than only the already-covered `running`/terminal
    /// cases.
    #[tokio::test]
    async fn reconnect_projects_every_intermediate_phase_not_just_running() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-reconnect-phases-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        {
            let database = journal.database().lock().await;
            let mut waiting_approval = sample_typed_ledger_event("event-phase-1", "action-phase-1");
            waiting_approval.state_after = Some(execution_ledger::ActionState::WaitingApproval);
            database
                .append_ledger_event(&waiting_approval)
                .expect("waiting_approval event appends");
            let mut cancelling = sample_typed_ledger_event("event-phase-2", "action-phase-2");
            cancelling.state_after = Some(execution_ledger::ActionState::Cancelling);
            database
                .append_ledger_event(&cancelling)
                .expect("cancelling event appends");
        }
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, mut server) = duplex(16 * 1024);

        bridge
            .push_journal_tail(&mut server, 0)
            .await
            .expect("tail pushes");

        let first = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("first frame reads")
                .as_slice(),
        )
        .expect("first frame decodes");
        let second = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("second frame reads")
                .as_slice(),
        )
        .expect("second frame decodes");

        for (envelope, expected_state) in [(first, "waiting_approval"), (second, "cancelling")] {
            let projected = match envelope.event {
                Some(generated::event_envelope::Event::ExecutionEvent(projected)) => projected,
                other => panic!("expected ExecutionEvent oneof, got {other:?}"),
            };
            assert_eq!(projected.state_after, expected_state);
        }
        let _ = std::fs::remove_file(&path);
    }

    /// Generic (non-`ledger.*`) rows keep flowing through the pre-08-3 path:
    /// `execution_event` stays unset and nothing else about the frame changes.
    #[tokio::test]
    async fn push_journal_tail_leaves_generic_rows_unprojected() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-generic-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "task-generic".into(),
                final_message: serde_json::json!({"ok": true}).to_string(),
            })
            .await
            .expect("generic event records");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, mut server) = duplex(16 * 1024);

        bridge
            .push_journal_tail(&mut server, 0)
            .await
            .expect("tail pushes");
        let frame = transport::read_frame(&mut client)
            .await
            .expect("frame reads");
        let envelope = generated::EventEnvelope::decode(frame.as_slice()).expect("frame decodes");

        assert!(
            envelope.event.is_none(),
            "generic row must not get a typed projection"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Regression: the clear marker was published on the coordinator broadcast,
    /// so the journal writer recorded it a moment later. The listing that the
    /// panel sends right after the response still read the old marker and kept
    /// showing the reviews that had just been cleared.
    #[tokio::test]
    async fn clearing_history_hides_reviews_from_the_next_listing() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-clear-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "review-old".into(),
                final_message: serde_json::json!({
                    "review_id": "review-old",
                    "file_name": "plan.md",
                    "synthesis_model": "main",
                    "reviewers": [],
                    "final_markdown": "# Итог"
                })
                .to_string(),
            })
            .await
            .expect("review records");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        let list_frame = || {
            generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: "review-list".into(),
                client_id: "test-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(generated::command_envelope::Command::ListPlanReviews(
                    generated::ListPlanReviews { limit: 20 },
                )),
            }
            .encode_to_vec()
        };

        transport::write_frame(&mut client, &list_frame())
            .await
            .expect("list writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("list serves");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("list response")
                .as_slice(),
        )
        .expect("list decodes");
        let before: serde_json::Value =
            serde_json::from_slice(&response.payload).expect("list json");
        assert_eq!(before["reviews"].as_array().expect("reviews").len(), 1);

        let clear = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "review-clear".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(
                generated::command_envelope::Command::ClearPlanReviewHistory(
                    generated::ClearPlanReviewHistory {},
                ),
            ),
        };
        transport::write_frame(&mut client, &clear.encode_to_vec())
            .await
            .expect("clear writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("clear serves");
        let _ = transport::read_frame(&mut client)
            .await
            .expect("clear response");

        // The panel lists again as soon as the clear is acknowledged.
        transport::write_frame(&mut client, &list_frame())
            .await
            .expect("second list writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("second list serves");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("second list response")
                .as_slice(),
        )
        .expect("second list decodes");
        let after: serde_json::Value =
            serde_json::from_slice(&response.payload).expect("list json");
        assert!(
            after["reviews"].as_array().expect("reviews").is_empty(),
            "a cleared history must be empty in the very next listing"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Regression: журнал вырос, снапшот resync перестал влезать в кадр IPC,
    /// и Core обрывал соединение с оболочкой. Оболочка переподключалась без
    /// состояния и навсегда показывала «нет связи с процессом слушателя»,
    /// хотя слушатель работал. Превышение лимита обязано деградировать до
    /// поштучной отправки, а не рвать канал.
    #[tokio::test]
    async fn an_oversized_snapshot_degrades_instead_of_dropping_the_shell() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-snapshot-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        // Payload журнала уезжает в снапшот массивом чисел, поэтому байты
        // раздуваются в несколько раз: восьми записей хватает, чтобы перейти
        // границу кадра.
        for index in 0..8 {
            journal
                .record(&CoreEvent::TaskCompleted {
                    task_id: format!("task-{index}"),
                    final_message: "a".repeat(200 * 1024),
                })
                .await
                .expect("event records");
        }
        let bridge = IpcBridge::new(journal);
        let (client, server) = duplex(64 * 1024 * 1024);
        let (mut client_reader, mut client_writer) = tokio::io::split(client);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        let request = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "resync-1".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 0,
            command: Some(generated::command_envelope::Command::ResyncRequest(
                generated::ResyncRequest {
                    after_sequence: 0,
                    max_events: 0,
                    include_full_snapshot: true,
                },
            )),
        };
        transport::write_frame(&mut client_writer, &request.encode_to_vec())
            .await
            .expect("resync writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("оболочка не должна терять соединение из-за размера снапшота");

        let mut seen = Vec::new();
        loop {
            let frame = transport::read_frame(&mut client_reader)
                .await
                .expect("resync response");
            let event = generated::EventEnvelope::decode(frame.as_slice()).expect("event decodes");
            seen.push(event.event_type.clone());
            if event.event_type == "resync.end" {
                break;
            }
        }

        assert!(
            seen.iter().any(|event| event == "replay.snapshot_skipped"),
            "оболочку нужно предупредить о пропущенном снапшоте: {seen:?}"
        );
        assert!(
            !seen.iter().any(|event| event == "replay.full_snapshot"),
            "снапшот сверх лимита отправлять нельзя: {seen:?}"
        );
        assert_eq!(
            seen.len(),
            10,
            "вместо снапшота оболочка обязана получить те же события поштучно: {seen:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// A large backlog is paged `max_events` at a time (план про «нет связи»
    /// после большой сессии): `resync.end` must say when more history sits
    /// beyond the page it just sent, so the shell chains the next resync
    /// itself instead of racing a random live-event gap to notice.
    #[tokio::test]
    async fn resync_end_reports_more_available_across_a_bounded_page() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-more-available-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        for index in 0..3 {
            journal
                .record(&CoreEvent::TaskCompleted {
                    task_id: format!("task-{index}"),
                    final_message: "done".into(),
                })
                .await
                .expect("event records");
        }
        let bridge = IpcBridge::new(journal);
        let (client, server) = duplex(64 * 1024);
        let (mut client_reader, mut client_writer) = tokio::io::split(client);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        let request = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "resync-page-1".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 0,
            command: Some(generated::command_envelope::Command::ResyncRequest(
                generated::ResyncRequest {
                    after_sequence: 0,
                    max_events: 2,
                    include_full_snapshot: false,
                },
            )),
        };
        transport::write_frame(&mut client_writer, &request.encode_to_vec())
            .await
            .expect("resync writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("resync succeeds");

        let end = loop {
            let frame = transport::read_frame(&mut client_reader)
                .await
                .expect("resync response");
            let event = generated::EventEnvelope::decode(frame.as_slice()).expect("event decodes");
            if event.event_type == "resync.end" {
                break event;
            }
        };
        assert_eq!(end.sequence_id, 2, "page stops at the requested max_events");
        let payload: serde_json::Value =
            serde_json::from_slice(&end.payload).expect("resync.end payload decodes as json");
        assert_eq!(
            payload["more_available"],
            serde_json::json!(true),
            "a third event sits beyond this page: {payload:?}"
        );
        assert_eq!(payload["latest_sequence"], serde_json::json!(3));

        let _ = std::fs::remove_file(&path);
    }

    /// The revised plan is written by Core, not by the shell, so the extension
    /// guard lives here: a shell bug must not be able to overwrite a `.rs` or a
    /// `.json` with Markdown.
    #[tokio::test]
    async fn saves_a_revised_plan_only_to_a_markdown_path() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-revision-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let bridge = IpcBridge::new(journal);
        bridge.revision_results.lock().await.insert(
            "revision-1".into(),
            crate::plan_review::RevisionResult {
                revision_id: "revision-1".into(),
                review_id: "review-1".into(),
                file_name: "plan.md".into(),
                model: "main".into(),
                revised_markdown: "# Исправленный план".into(),
                context_files: Vec::new(),
            },
        );
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let save = |destination: &str| {
            generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: "revision-save".into(),
                client_id: "test-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(generated::command_envelope::Command::SaveRevisedPlan(
                    generated::SaveRevisedPlan {
                        revision_id: "revision-1".into(),
                        destination_path: destination.into(),
                    },
                )),
            }
            .encode_to_vec()
        };

        let destination =
            std::env::temp_dir().join(format!("evohime-revised-{}.md", std::process::id()));
        let _ = std::fs::remove_file(&destination);
        transport::write_frame(&mut client, &save(&destination.to_string_lossy()))
            .await
            .expect("save writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("save serves");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("save response")
                .as_slice(),
        )
        .expect("save decodes");
        assert_eq!(response.event_type, "plan.saved");
        assert_eq!(
            std::fs::read_to_string(&destination).expect("revised plan is on disk"),
            "# Исправленный план"
        );

        // Отказ приходит событием: ошибка кадра оборвала бы соединение с
        // оболочкой, и опечатка в имени файла читалась бы как падение ядра.
        let rejected = std::env::temp_dir().join("evohime-revised.txt");
        transport::write_frame(&mut client, &save(&rejected.to_string_lossy()))
            .await
            .expect("rejected save writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("a refused save keeps the connection");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("refusal response")
                .as_slice(),
        )
        .expect("refusal decodes");
        assert_eq!(response.event_type, "plan.save_failed");
        assert!(!rejected.exists());

        let _ = std::fs::remove_file(&destination);
        let _ = std::fs::remove_file(&path);
    }

    /// Обновление Евы перезапускает Core, а нажать «сохранить» пользователь
    /// может и после этого: правка обязана находиться в журнале, когда кэш уже
    /// пуст.
    #[tokio::test]
    async fn saves_a_revised_plan_recovered_from_the_journal() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-revision-journal-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "revision-7".into(),
                final_message: serde_json::json!({
                    "revision_id": "revision-7",
                    "review_id": "review-1",
                    "file_name": "plan.md",
                    "model": "main",
                    "revised_markdown": "# Восстановленный план"
                })
                .to_string(),
            })
            .await
            .expect("revision records");
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let destination =
            std::env::temp_dir().join(format!("evohime-revised-journal-{}.md", std::process::id()));
        let _ = std::fs::remove_file(&destination);
        transport::write_frame(
            &mut client,
            &generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: "revision-save".into(),
                client_id: "test-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(generated::command_envelope::Command::SaveRevisedPlan(
                    generated::SaveRevisedPlan {
                        revision_id: "revision-7".into(),
                        destination_path: destination.to_string_lossy().into(),
                    },
                )),
            }
            .encode_to_vec(),
        )
        .await
        .expect("save writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("save serves");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("save response")
                .as_slice(),
        )
        .expect("save decodes");
        assert_eq!(response.event_type, "plan.saved");
        assert_eq!(
            std::fs::read_to_string(&destination).expect("revised plan is on disk"),
            "# Восстановленный план"
        );
        let _ = std::fs::remove_file(&destination);
        let _ = std::fs::remove_file(&path);
    }

    /// Revising a review the core has never seen would let the shell hand the
    /// editing model an arbitrary text and call it a review.
    #[tokio::test]
    async fn refuses_to_revise_an_unknown_review() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-revision-missing-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "revision-start".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::RevisePlan(
                generated::RevisePlan {
                    revision_id: "revision-1".into(),
                    review_id: "review-missing".into(),
                    file_name: "plan.md".into(),
                    source_markdown: "# Plan".into(),
                    model: "main".into(),
                    source_path: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("revise writes");
        assert!(bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn serves_replay_command_over_framed_transport() {
        let path = std::env::temp_dir().join(format!("evohime-ipc-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "task-ipc".into(),
                final_message: "replayed".into(),
            })
            .await
            .expect("event records");
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "request-1".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 0,
            command: Some(generated::command_envelope::Command::ReplayEvents(
                generated::ReplayEvents { after_sequence: 0 },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("bridge serves replay");
        let response = transport::read_frame(&mut client)
            .await
            .expect("response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.sequence_id, 1);
        assert_eq!(event.task_id, "task-ipc");
        assert_eq!(event.event_type, "task.completed");
        assert!(String::from_utf8(event.payload)
            .expect("payload utf8")
            .contains("replayed"));
        let _ = std::fs::remove_file(path);
    }

    /// План 08-4: `redaction.secrets_present` must reflect a real scan of
    /// the request, not just always be `false` — the same secret-shape
    /// markers `crate::audit::contains_secret` already redacts on.
    #[test]
    fn tool_request_redaction_flags_secret_shaped_input_and_clears_ordinary_input() {
        let secret_request = evohime_receipts::runtime::ActionRequest {
            action_id: uuid::Uuid::now_v7(),
            task_id: "task-1".into(),
            run_id: "task-1".into(),
            tool_name: "shell.execute".into(),
            policy_id: "permission:ShellExecute".into(),
            normalized_scope: "workspace".into(),
            input: serde_json::json!({"program": "curl", "args": ["-H", "Authorization: Bearer sk-abc123"]}),
            policy_decision: evohime_receipts::runtime::PolicyDecision::Allow,
            approval_id: None,
            parent_approval_ref: None,
            preview: "curl call".into(),
        };
        assert!(tool_request_redaction(&secret_request).secrets_present);

        let ordinary_request = evohime_receipts::runtime::ActionRequest {
            input: serde_json::json!({"program": "git", "args": ["status"]}),
            ..secret_request
        };
        assert!(!tool_request_redaction(&ordinary_request).secrets_present);
    }

    /// План 08-3: a client whose `CommandEnvelope` names a different
    /// generation than this process must get an honest typed `ReplayGap`
    /// with `reason = "stale_generation"` before the (still-served) replay,
    /// not just silently receive events stamped with a new identity.
    #[tokio::test]
    async fn stale_generation_produces_a_typed_replay_gap_before_replay() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-stale-generation-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "task-stale".into(),
                final_message: "stale".into(),
            })
            .await
            .expect("event records");
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "request-stale".into(),
            client_id: "test-client".into(),
            core_instance_id: "a-generation-this-process-never-had".into(),
            session_epoch: 0,
            command: Some(generated::command_envelope::Command::ReplayEvents(
                generated::ReplayEvents { after_sequence: 0 },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("bridge serves replay");

        let gap_frame = transport::read_frame(&mut client)
            .await
            .expect("gap frame reads");
        let gap_envelope =
            generated::EventEnvelope::decode(gap_frame.as_slice()).expect("gap decodes");
        let gap = match gap_envelope.event {
            Some(generated::event_envelope::Event::ReplayGap(gap)) => gap,
            other => panic!("expected typed ReplayGap, got {other:?}"),
        };
        assert_eq!(gap.reason, "stale_generation");
        assert_eq!(gap.requested_after_sequence, 0);

        let event_frame = transport::read_frame(&mut client)
            .await
            .expect("event frame reads");
        let event =
            generated::EventEnvelope::decode(event_frame.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "task.completed");
        let _ = std::fs::remove_file(path);
    }

    /// План 08-3 п.5: `FullSnapshot.snapshot_json` carries a bounded typed
    /// action projection (latest state per `action_id`), not just a raw
    /// event dump — a reconnecting client can rebuild action cards from the
    /// snapshot alone.
    #[tokio::test]
    async fn resync_snapshot_includes_typed_action_projection() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-snapshot-actions-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let ledger_event = execution_ledger::ExecutionEventV1 {
            schema_version: 1,
            event_id: "event-snapshot-action-1".into(),
            sequence_id: None,
            run_scope: execution_ledger::RunScope::Standalone,
            run_id: "run-snapshot-1".into(),
            session_id: Some("session-snapshot-1".into()),
            task_id: "task-snapshot".into(),
            created_at_ms: 1_700_000_000_000,
            state_after: Some(execution_ledger::ActionState::Running),
            action_id: Some("action-snapshot-1".into()),
            tool_call_id: None,
            observation_id: None,
            receipt_id: None,
            failure_id: None,
            workflow_run_id: None,
            node_id: None,
            attempt_id: None,
            effect_id: None,
            model_request_id: None,
            body: execution_ledger::ExecutionEventBody::ToolCall {
                tool_name: "shell".into(),
                tool_call_hash: "hash-1".into(),
                manifest_hash: None,
            },
            redaction: execution_ledger::RedactionMeta::default(),
        };
        {
            let database = journal.database().lock().await;
            database
                .append_ledger_event(&ledger_event)
                .expect("typed event appends");
        }
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "resync-actions".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 0,
            command: Some(generated::command_envelope::Command::ResyncRequest(
                generated::ResyncRequest {
                    after_sequence: 0,
                    max_events: 0,
                    include_full_snapshot: true,
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("resync writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("resync serves");

        let frame = transport::read_frame(&mut client)
            .await
            .expect("snapshot frame reads");
        let envelope = generated::EventEnvelope::decode(frame.as_slice()).expect("frame decodes");
        let snapshot = match envelope.event {
            Some(generated::event_envelope::Event::FullSnapshot(snapshot)) => snapshot,
            other => panic!("expected FullSnapshot, got {other:?}"),
        };
        let payload: serde_json::Value =
            serde_json::from_slice(&snapshot.snapshot_json).expect("snapshot json decodes");
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["snapshot_sequence_id"], snapshot.sequence_id);
        let actions = payload["actions"].as_array().expect("actions array");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0]["action_id"], "action-snapshot-1");
        assert_eq!(actions[0]["state_after"], "running");
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn serves_bounded_workspace_list_and_file_read_over_ipc() {
        let root =
            std::env::temp_dir().join(format!("evohime-ipc-workspace-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).expect("src directory");
        std::fs::write(root.join("README.md"), "hello from workspace").expect("readme");
        let journal_path =
            std::env::temp_dir().join(format!("evohime-ipc-workspace-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&journal_path);
        let bridge = IpcBridge::new(EventJournal::open(&journal_path).expect("journal opens"));
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        let list = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "workspace-list".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ListWorkspace(
                generated::ListWorkspace {
                    workspace_path: root.display().to_string(),
                    relative_path: ".".into(),
                    max_entries: 10,
                },
            )),
        };
        transport::write_frame(&mut client, &list.encode_to_vec())
            .await
            .expect("list writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("list serves");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("list reads")
                .as_slice(),
        )
        .expect("list event decodes");
        assert_eq!(response.event_type, "workspace.list");
        let listing: serde_json::Value =
            serde_json::from_slice(&response.payload).expect("list json");
        assert_eq!(listing["entries"][0]["name"], "src");

        let read = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "workspace-read".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ReadWorkspaceFile(
                generated::ReadWorkspaceFile {
                    workspace_path: root.display().to_string(),
                    relative_path: "README.md".into(),
                    max_bytes: 100,
                },
            )),
        };
        transport::write_frame(&mut client, &read.encode_to_vec())
            .await
            .expect("read writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("read serves");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("read response")
                .as_slice(),
        )
        .expect("read event decodes");
        assert_eq!(response.event_type, "workspace.file");
        let file: serde_json::Value = serde_json::from_slice(&response.payload).expect("file json");
        assert_eq!(file["content"], "hello from workspace");
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_file(journal_path);
    }

    #[tokio::test]
    async fn terminal_requires_approval_and_denied_retry_does_not_execute() {
        let root =
            std::env::temp_dir().join(format!("evohime-ipc-terminal-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("terminal root");
        let data_root =
            std::env::temp_dir().join(format!("evohime-ipc-terminal-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data_root);
        std::fs::create_dir_all(&data_root).expect("terminal data root");
        let journal_path = data_root.join("events.db");
        let _ = std::fs::remove_file(&journal_path);
        let receipt_keys = ReceiptKeyManager::new(&data_root);
        receipt_keys.initialize().expect("receipt keys initialize");
        let journal = EventJournal::open(&journal_path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let tools = Arc::new(ToolRegistry::bootstrap());
        let bridge = IpcBridge::with_coordinator_and_approvals(
            journal,
            coordinator,
            ApprovalCoordinator::default(),
            tools,
            None,
            None,
        );
        let task_id = uuid::Uuid::new_v4().to_string();
        let make_terminal = |approval_id: String| generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "terminal-request".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::TerminalExecute(
                generated::TerminalExecute {
                    task_id: task_id.clone(),
                    workspace_path: root.display().to_string(),
                    program: "git".into(),
                    args: vec!["status".into()],
                    cwd: String::new(),
                    timeout_ms: 5_000,
                    approval_id,
                },
            )),
        };
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        transport::write_frame(&mut client, &make_terminal(String::new()).encode_to_vec())
            .await
            .expect("terminal writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("approval serves");
        let approval = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("approval reads")
                .as_slice(),
        )
        .expect("approval decodes");
        assert_eq!(approval.event_type, "approval.required");
        let approval_json =
            serde_json::from_slice::<serde_json::Value>(&approval.payload).expect("approval json");
        assert_eq!(approval_json["preview"]["kind"], "command");
        assert_eq!(approval_json["preview"]["command"], "git status");
        let approval_id = approval_json["approval_id"]
            .as_str()
            .expect("approval id")
            .to_string();

        let resolve = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "resolve-terminal".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ResolveApproval(
                generated::ResolveApproval {
                    approval_id: approval_id.clone(),
                    granted: false,
                    idempotency_key: String::new(),
                    rejection_reason: String::new(),
                    cancel: false,
                },
            )),
        };
        transport::write_frame(&mut client, &resolve.encode_to_vec())
            .await
            .expect("resolve writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("resolve serves");

        // План 08-4 acceptance: a denied approval publishes a typed
        // ApprovalDecision/Denied ledger event linked to the receipts
        // approval intent's own action_id — this is the "reject" arm of
        // "approval approve/reject/expiry".
        {
            let journal_handle = bridge.journal();
            let database = journal_handle.database().lock().await;
            let (decision_state, body_payload): (String, Vec<u8>) = database
                .connection()
                .query_row(
                    "SELECT state_after, payload FROM events
                       WHERE event_type = 'ledger.approval_decision'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("ledger.approval_decision row exists");
            assert_eq!(decision_state, "denied");
            let decision_event: execution_ledger::ExecutionEventV1 =
                serde_json::from_slice(&body_payload).expect("decision event decodes");
            let execution_ledger::ExecutionEventBody::ApprovalDecision {
                approval_intent_id,
                decision,
                ..
            } = decision_event.body
            else {
                panic!(
                    "expected ApprovalDecision body, got {:?}",
                    decision_event.body
                );
            };
            assert_eq!(approval_intent_id, approval_id);
            assert_eq!(decision, execution_ledger::ApprovalOutcome::Rejected);
        }

        transport::write_frame(&mut client, &make_terminal(approval_id).encode_to_vec())
            .await
            .expect("retry writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("retry serves");
        let result = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("result reads")
                .as_slice(),
        )
        .expect("result decodes");
        assert_eq!(result.event_type, "terminal.result");
        let result_json: serde_json::Value =
            serde_json::from_slice(&result.payload).expect("result json");
        assert_eq!(result_json["ok"], false);
        assert_eq!(result_json["error_code"], "approval_denied");
        assert!(result_json.get("error").is_none());
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(data_root);
    }

    /// План 08-4 acceptance: the third arm of "approval approve/reject/
    /// expiry". A retry that arrives after the approval window closed must
    /// be refused by `grant_approval`'s own deadline check (not by a new
    /// check invented here) and publish a typed `ApprovalDecision/Expired`
    /// ledger event before the error propagates. The deadline is forced
    /// into the past directly in `receipt_approval_intents` — waiting out
    /// the real 10-minute TTL is not a workable test.
    #[tokio::test]
    async fn expired_approval_publishes_ledger_decision_and_refuses_the_retry() {
        let root = std::env::temp_dir().join(format!(
            "evohime-ipc-terminal-expiry-root-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("terminal root");
        let data_root = std::env::temp_dir().join(format!(
            "evohime-ipc-terminal-expiry-data-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_root);
        std::fs::create_dir_all(&data_root).expect("terminal data root");
        let journal_path = data_root.join("events.db");
        let _ = std::fs::remove_file(&journal_path);
        let receipt_keys = ReceiptKeyManager::new(&data_root);
        receipt_keys.initialize().expect("receipt keys initialize");
        let journal = EventJournal::open(&journal_path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let tools = Arc::new(ToolRegistry::bootstrap());
        let bridge = IpcBridge::with_coordinator_and_approvals(
            journal.clone(),
            coordinator,
            ApprovalCoordinator::default(),
            tools,
            None,
            None,
        );
        let task_id = uuid::Uuid::new_v4().to_string();
        let make_terminal = |approval_id: String| generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "terminal-request".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::TerminalExecute(
                generated::TerminalExecute {
                    task_id: task_id.clone(),
                    workspace_path: root.display().to_string(),
                    program: "git".into(),
                    args: vec!["status".into()],
                    cwd: String::new(),
                    timeout_ms: 5_000,
                    approval_id,
                },
            )),
        };
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        transport::write_frame(&mut client, &make_terminal(String::new()).encode_to_vec())
            .await
            .expect("terminal writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("approval serves");
        let approval = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("approval reads")
                .as_slice(),
        )
        .expect("approval decodes");
        let approval_json =
            serde_json::from_slice::<serde_json::Value>(&approval.payload).expect("approval json");
        let approval_id = approval_json["approval_id"]
            .as_str()
            .expect("approval id")
            .to_string();

        // Force the approval window into the past — same-process retries
        // hit the monotonic-clock branch of `grant_approval`'s deadline
        // check, so backdating `deadline_monotonic_ms` is what actually
        // exercises it (backdating `expires_at_ms` alone would not, since
        // the boot id matches).
        {
            let database = journal.database().lock().await;
            let changed = database
                .connection()
                .execute(
                    "UPDATE receipt_approval_intents SET deadline_monotonic_ms = 0 WHERE approval_id = ?1",
                    [&approval_id],
                )
                .expect("deadline backdates");
            assert_eq!(changed, 1, "the approval intent row must exist");
        }

        transport::write_frame(
            &mut client,
            &make_terminal(approval_id.clone()).encode_to_vec(),
        )
        .await
        .expect("retry writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect_err("an expired approval must refuse the retry");

        let journal_handle = bridge.journal();
        let database = journal_handle.database().lock().await;
        let (decision_state, body_payload): (String, Vec<u8>) = database
            .connection()
            .query_row(
                "SELECT state_after, payload FROM events
                   WHERE event_type = 'ledger.approval_decision'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ledger.approval_decision row exists");
        assert_eq!(decision_state, "timed_out");
        let decision_event: execution_ledger::ExecutionEventV1 =
            serde_json::from_slice(&body_payload).expect("decision event decodes");
        let execution_ledger::ExecutionEventBody::ApprovalDecision {
            approval_intent_id,
            decision,
            ..
        } = decision_event.body
        else {
            panic!(
                "expected ApprovalDecision body, got {:?}",
                decision_event.body
            );
        };
        assert_eq!(approval_intent_id, approval_id);
        assert_eq!(decision, execution_ledger::ApprovalOutcome::Expired);
        drop(database);

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(data_root);
    }

    /// План 08-4 acceptance: "action → tool call → observation → successful
    /// typed receipt linked to signed receipts_v1". A real terminal
    /// execution, approved and run through `dispatch_terminal_execute`, must
    /// leave a typed `ledger.tool_call` (Running) followed by a typed
    /// `ledger.tool_receipt` (Succeeded) under the same `action_id` — and
    /// that receipt event's `receipt_hash` must resolve to an actual signed
    /// row in `receipt_records`, not just a plausible-looking string.
    #[tokio::test]
    async fn approved_terminal_execute_links_ledger_receipt_to_signed_receipts_v1() {
        let root = std::env::temp_dir().join(format!(
            "evohime-ipc-terminal-linkage-root-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("terminal root");
        std::process::Command::new("git")
            .arg("init")
            .arg(&root)
            .output()
            .expect("git init runs");
        let data_root = std::env::temp_dir().join(format!(
            "evohime-ipc-terminal-linkage-data-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_root);
        std::fs::create_dir_all(&data_root).expect("terminal data root");
        let journal_path = data_root.join("events.db");
        let _ = std::fs::remove_file(&journal_path);
        let receipt_keys = ReceiptKeyManager::new(&data_root);
        receipt_keys.initialize().expect("receipt keys initialize");
        let journal = EventJournal::open(&journal_path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let tools = Arc::new(ToolRegistry::bootstrap());
        let bridge = IpcBridge::with_coordinator_and_approvals(
            journal.clone(),
            coordinator,
            ApprovalCoordinator::default(),
            tools,
            None,
            None,
        );
        let task_id = uuid::Uuid::new_v4().to_string();
        let make_terminal = |approval_id: String| generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "terminal-request".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::TerminalExecute(
                generated::TerminalExecute {
                    task_id: task_id.clone(),
                    workspace_path: root.display().to_string(),
                    program: "git".into(),
                    args: vec!["status".into()],
                    cwd: String::new(),
                    timeout_ms: 5_000,
                    approval_id,
                },
            )),
        };
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        transport::write_frame(&mut client, &make_terminal(String::new()).encode_to_vec())
            .await
            .expect("terminal writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("approval serves");
        let approval = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("approval reads")
                .as_slice(),
        )
        .expect("approval decodes");
        let approval_json =
            serde_json::from_slice::<serde_json::Value>(&approval.payload).expect("approval json");
        let approval_id = approval_json["approval_id"]
            .as_str()
            .expect("approval id")
            .to_string();

        let resolve = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "resolve-terminal".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ResolveApproval(
                generated::ResolveApproval {
                    approval_id: approval_id.clone(),
                    granted: true,
                    idempotency_key: String::new(),
                    rejection_reason: String::new(),
                    cancel: false,
                },
            )),
        };
        transport::write_frame(&mut client, &resolve.encode_to_vec())
            .await
            .expect("resolve writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("resolve serves");

        // План 08-4 acceptance: a granted approval publishes a typed
        // ApprovalDecision/Approved ledger event — the "approve" arm of
        // "approval approve/reject/expiry" — before the retried execution
        // publishes its own ToolCall/ToolReceipt pair below.
        {
            let database = journal.database().lock().await;
            let (decision_state, body_payload): (String, Vec<u8>) = database
                .connection()
                .query_row(
                    "SELECT state_after, payload FROM events
                       WHERE event_type = 'ledger.approval_decision'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("ledger.approval_decision row exists");
            assert_eq!(decision_state, "running");
            let decision_event: execution_ledger::ExecutionEventV1 =
                serde_json::from_slice(&body_payload).expect("decision event decodes");
            let execution_ledger::ExecutionEventBody::ApprovalDecision {
                approval_intent_id,
                decision,
                ..
            } = decision_event.body
            else {
                panic!(
                    "expected ApprovalDecision body, got {:?}",
                    decision_event.body
                );
            };
            assert_eq!(approval_intent_id, approval_id);
            assert_eq!(decision, execution_ledger::ApprovalOutcome::Approved);
        }

        transport::write_frame(&mut client, &make_terminal(approval_id).encode_to_vec())
            .await
            .expect("retry writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("retry serves");
        let result = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("result reads")
                .as_slice(),
        )
        .expect("result decodes");
        assert_eq!(result.event_type, "terminal.result");
        let result_json: serde_json::Value =
            serde_json::from_slice(&result.payload).expect("result json");
        assert_eq!(
            result_json["ok"], true,
            "git status in a real repo must succeed: {result_json}"
        );

        let database = journal.database().lock().await;
        let (tool_call_action_id, tool_call_state): (String, String) = database
            .connection()
            .query_row(
                "SELECT action_id, state_after FROM events WHERE event_type = 'ledger.tool_call'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ledger.tool_call row exists");
        assert_eq!(tool_call_state, "running");

        // The "observation" link of "action → tool call → observation →
        // receipt" — must exist under the same action_id, between the call
        // and the receipt.
        let (observation_action_id, observation_payload): (String, Vec<u8>) = database
            .connection()
            .query_row(
                "SELECT action_id, payload FROM events WHERE event_type = 'ledger.observation'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ledger.observation row exists");
        assert_eq!(observation_action_id, tool_call_action_id);
        let observation_event: execution_ledger::ExecutionEventV1 =
            serde_json::from_slice(&observation_payload).expect("observation event decodes");
        assert!(matches!(
            observation_event.body,
            execution_ledger::ExecutionEventBody::Observation { .. }
        ));

        let (receipt_action_id, receipt_state, receipt_payload): (String, String, Vec<u8>) =
            database
                .connection()
                .query_row(
                    "SELECT action_id, state_after, payload FROM events
                       WHERE event_type = 'ledger.tool_receipt'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .expect("ledger.tool_receipt row exists");
        assert_eq!(receipt_state, "succeeded");
        assert_eq!(
            receipt_action_id, tool_call_action_id,
            "tool_call and tool_receipt must share the same action_id"
        );
        let receipt_event: execution_ledger::ExecutionEventV1 =
            serde_json::from_slice(&receipt_payload).expect("receipt event decodes");
        let execution_ledger::ExecutionEventBody::ToolReceipt {
            receipt_action_id: body_action_id,
            receipt_hash,
        } = receipt_event.body
        else {
            panic!("expected ToolReceipt body, got {:?}", receipt_event.body);
        };
        assert_eq!(body_action_id, receipt_action_id);

        // The linkage is only real if that hash resolves to an actual signed
        // row — not merely a string that looks like one.
        let signed_action_id: String = database
            .connection()
            .query_row(
                "SELECT action_id FROM receipt_records WHERE receipt_hash = ?1",
                [&receipt_hash],
                |row| row.get(0),
            )
            .expect("receipt_hash resolves to a real receipt_records row");
        assert_eq!(signed_action_id, receipt_action_id);
        drop(database);

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(data_root);
    }

    #[tokio::test]
    async fn reconciliation_command_executes_only_new_read_only_action() {
        let root =
            std::env::temp_dir().join(format!("evohime-ipc-reconcile-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("reconcile root");
        std::fs::write(root.join("observed.txt"), "observed state\n").expect("observed file");
        let data_root =
            std::env::temp_dir().join(format!("evohime-ipc-reconcile-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data_root);
        std::fs::create_dir_all(&data_root).expect("reconcile data root");
        let journal_path = data_root.join("events.db");
        let keys = ReceiptKeyManager::new(&data_root);
        keys.initialize().expect("keys initialize");
        let journal = EventJournal::open(&journal_path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator_and_approvals(
            journal.clone(),
            coordinator,
            ApprovalCoordinator::default(),
            Arc::new(ToolRegistry::bootstrap()),
            None,
            None,
        );
        let task_id = uuid::Uuid::new_v4();
        let old_action_id = uuid::Uuid::now_v7();
        {
            let mut database = journal.database().lock().await;
            let signer = crate::CoreReceiptSigner(Arc::new(keys));
            let mut runtime =
                evohime_receipts::runtime::ReceiptRuntime::new(database.connection_mut(), &signer)
                    .unwrap();
            let old_request = evohime_receipts::runtime::ActionRequest {
                action_id: old_action_id,
                task_id: task_id.to_string(),
                run_id: task_id.to_string(),
                tool_name: "shell.execute".into(),
                policy_id: "permission:ShellExecute".into(),
                normalized_scope: "workspace".into(),
                input: serde_json::json!({"program":"echo","args":[]}),
                policy_decision: evohime_receipts::runtime::PolicyDecision::Allow,
                approval_id: None,
                parent_approval_ref: None,
                preview: "old mutation".into(),
            };
            runtime.prepare(old_request).unwrap();
            runtime.mark_started(old_action_id).unwrap();
            runtime.mark_returned(old_action_id).unwrap();
            runtime
                .mark_pending_recovery(old_action_id, "unknown")
                .unwrap();
        }
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "reconcile-read-only".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(
                generated::command_envelope::Command::ReconcilePendingReceiptAction(
                    generated::ReconcilePendingReceiptAction {
                        old_action_id: old_action_id.to_string(),
                        tool_name: "filesystem.read".into(),
                        input_json: r#"{"path":"observed.txt"}"#.into(),
                        workspace_path: root.display().to_string(),
                    },
                ),
            ),
        };
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .unwrap();
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .unwrap();
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client).await.unwrap().as_slice(),
        )
        .unwrap();
        assert_eq!(response.event_type, "receipt.reconciliation");
        let payload: serde_json::Value = serde_json::from_slice(&response.payload).unwrap();
        assert_eq!(payload["ok"], true);
        assert_eq!(payload["status"], "succeeded");
        assert_ne!(payload["action_id"], old_action_id.to_string());
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&data_root);
    }

    #[tokio::test]
    async fn serves_bounded_git_status_and_diff_through_core_tools() {
        let root =
            std::env::temp_dir().join(format!("evohime-ipc-git-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("git root");
        let status = std::process::Command::new("git")
            .args(["init"])
            .current_dir(&root)
            .status()
            .expect("git init starts");
        assert!(status.success());
        std::fs::write(root.join("notes.txt"), "hello\n").expect("notes");
        let status = std::process::Command::new("git")
            .args(["add", "notes.txt"])
            .current_dir(&root)
            .status()
            .expect("git add starts");
        assert!(status.success());
        std::fs::write(root.join("notes.txt"), "hello\nworld\n").expect("changed notes");
        let journal_path =
            std::env::temp_dir().join(format!("evohime-ipc-git-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&journal_path);
        let journal = EventJournal::open(&journal_path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let tools = Arc::new(ToolRegistry::bootstrap());
        let bridge = IpcBridge::with_coordinator_and_approvals(
            journal,
            coordinator,
            ApprovalCoordinator::default(),
            tools,
            None,
            None,
        );

        let status_payload = bridge
            .dispatch_git_read(
                root.display().to_string(),
                "git.status",
                serde_json::Value::Null,
                128,
            )
            .await
            .expect("git status reads");
        let status_json: serde_json::Value =
            serde_json::from_slice(&status_payload).expect("status json");
        assert!(status_json["output"]
            .as_str()
            .unwrap()
            .contains("notes.txt"));
        assert_eq!(status_json["truncated"], false);

        let diff_payload = bridge
            .dispatch_git_read(
                root.display().to_string(),
                "git.diff",
                serde_json::json!({"path": "notes.txt"}),
                8,
            )
            .await
            .expect("git diff reads");
        let diff_json: serde_json::Value =
            serde_json::from_slice(&diff_payload).expect("diff json");
        assert_eq!(diff_json["max_bytes"], 8);
        assert_eq!(diff_json["truncated"], true);

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_file(journal_path);
    }

    #[tokio::test]
    async fn conversation_event_log_ipc_pages_and_live_events_are_typed_and_redacted() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("conversation-ipc.db");
        let journal = EventJournal::open(&path).expect("journal opens");
        journal
            .accept_conversation_message(
                "conversation-1",
                "workspace-1",
                "task-1",
                "client-1",
                "token sk-12345678901234567890",
            )
            .await
            .expect("message accepts");
        let bridge = IpcBridge::new(journal);
        let page = bridge
            .dispatch_conversation_event_log(
                generated::ConversationEventLogRequest {
                    schema_version: 1,
                    conversation_id: "conversation-1".into(),
                    before_sequence: 0,
                    after_sequence: 0,
                    use_before_sequence: false,
                    use_after_sequence: true,
                    limit: 20,
                    kinds_filter: Vec::new(),
                },
                "subscribed",
            )
            .await;
        assert!(page.error_code.is_empty());
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].sequence, 1);
        assert_eq!(page.events[0].client_message_id, "client-1");
        assert!(!String::from_utf8_lossy(&page.events[0].payload_json)
            .contains("sk-12345678901234567890"));

        let (mut client, mut server) = duplex(64 * 1024);
        bridge
            .push_journal_tail(&mut server, 0)
            .await
            .expect("live tail writes");
        let frame = transport::read_frame(&mut client)
            .await
            .expect("live frame reads");
        let envelope =
            generated::EventEnvelope::decode(frame.as_slice()).expect("live frame decodes");
        let Some(generated::event_envelope::Event::ConversationEventLog(live)) = envelope.event
        else {
            panic!("typed conversation event missing");
        };
        assert_eq!(live.operation, "live");
        assert_eq!(live.events[0].event_id, page.events[0].event_id);
    }

    #[tokio::test]
    async fn start_task_retry_is_idempotent_and_conflict_returns_typed_non_success() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-conversation-start-{}-{}.db",
            std::process::id(),
            uuid::Uuid::now_v7()
        ));
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, mut events) =
            TaskCoordinator::new_with_journal(16, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(64 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = |request_id: &str, prompt: &str| generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: request_id.into(),
            client_id: "client".into(),
            core_instance_id: String::new(),
            session_epoch: 0,
            command: Some(generated::command_envelope::Command::StartTask(
                generated::StartTask {
                    task_id: "task-conversation".into(),
                    prompt: prompt.into(),
                    workspace_path: ".".into(),
                    preferred_route_hint: "cloud".into(),
                    execution_kind: "dialogue".into(),
                    conversation_id: "conversation-1".into(),
                    client_message_id: "client-message-1".into(),
                },
            )),
        };

        transport::write_frame(&mut client, &command("start-1", "same").encode_to_vec())
            .await
            .expect("first command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("first command serves");
        assert!(matches!(
            events.recv().await,
            Ok(CoreEvent::TaskStarted { .. })
        ));

        transport::write_frame(&mut client, &command("start-retry", "same").encode_to_vec())
            .await
            .expect("retry writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("retry serves");

        transport::write_frame(
            &mut client,
            &command("start-conflict", "different").encode_to_vec(),
        )
        .await
        .expect("conflict writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("conflict serves");
        let frame = transport::read_frame(&mut client)
            .await
            .expect("typed rejection reads");
        let envelope =
            generated::EventEnvelope::decode(frame.as_slice()).expect("rejection decodes");
        let Some(generated::event_envelope::Event::ConversationEventLog(result)) = envelope.event
        else {
            panic!("conversation rejection missing");
        };
        assert_eq!(result.operation, "accept");
        assert_eq!(result.error_code, "idempotency_conflict");

        let mut duplicate_started = false;
        while let Ok(Ok(event)) =
            tokio::time::timeout(std::time::Duration::from_millis(50), events.recv()).await
        {
            duplicate_started |= matches!(event, CoreEvent::TaskStarted { .. });
        }
        assert!(
            !duplicate_started,
            "retry or conflict dispatched a second task"
        );
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn handshake_exposes_runtime_identity() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-handshake-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let bridge = IpcBridge::new(EventJournal::open(&path).expect("journal opens"));
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "handshake".into(),
            client_id: "client".into(),
            core_instance_id: String::new(),
            session_epoch: 9,
            command: Some(generated::command_envelope::Command::Handshake(
                generated::Handshake {
                    protocol: Some(protocol()),
                    client_id: "client".into(),
                    session_id: "session".into(),
                    session_epoch: 9,
                    last_event_sequence: 0,
                    capabilities: vec!["task.crud".into()],
                    client_role: "shell".into(),
                    nonce: String::new(),
                    proof: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("handshake writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("handshake serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert!(!event.core_instance_id.is_empty());
        assert!(event.session_epoch > 0);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn malformed_command_is_rejected_without_crashing_bridge() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-malformed-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let bridge = IpcBridge::new(EventJournal::open(&path).expect("journal opens"));
        let (mut client, server) = duplex(1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        transport::write_frame(&mut client, &[0xff, 0x00, 0x01])
            .await
            .expect("malformed frame writes");
        assert!(matches!(
            bridge
                .process_once(&mut server_reader, &mut server_writer)
                .await,
            Err(IpcBridgeError::Protobuf(_))
        ));
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn reconnect_replays_only_events_after_last_sequence() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-reconnect-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let first = journal
            .record(&CoreEvent::TaskStarted {
                task_id: "task-reconnect".into(),
                prompt: "one".into(),
            })
            .await
            .expect("first event");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "task-reconnect".into(),
                final_message: "two".into(),
            })
            .await
            .expect("second event");
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "reconnect".into(),
            client_id: "client".into(),
            core_instance_id: String::new(),
            session_epoch: 0,
            command: Some(generated::command_envelope::Command::ReplayEvents(
                generated::ReplayEvents {
                    after_sequence: first as u64,
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("reconnect writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("reconnect serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("event reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "task.completed");
        assert_eq!(event.sequence_id, first as u64 + 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn serves_task_crud_and_replays_deduplicated_create() {
        let path = std::env::temp_dir().join(format!("evohime-ipc-task-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "create-project-1".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::CreateProject(
                generated::CreateProject {
                    project_id: "project-1".into(),
                    title: "Demo".into(),
                    workspace_path: "C:\\Projects\\demo".into(),
                    source_ref: "plan:0a".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("project creates");
        let first = transport::read_frame(&mut client)
            .await
            .expect("first response");

        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("duplicate writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("duplicate replays");
        let second = transport::read_frame(&mut client)
            .await
            .expect("second response");
        assert_eq!(first, second);

        let mut conflict = command.clone();
        if let Some(generated::command_envelope::Command::CreateProject(project)) =
            &mut conflict.command
        {
            project.title = "Different".into();
        }
        transport::write_frame(&mut client, &conflict.encode_to_vec())
            .await
            .expect("conflicting writes");
        assert!(bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .is_err());
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn imports_prd_without_touching_workspace_and_rejects_duplicate_import() {
        let path = std::env::temp_dir().join(format!("evohime-ipc-prd-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server) = duplex(32 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let project = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "project-prd".into(),
            client_id: "prd-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::CreateProject(
                generated::CreateProject {
                    project_id: "project-prd".into(),
                    title: "PRD".into(),
                    workspace_path: "C:\\Projects\\prd".into(),
                    source_ref: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &project.encode_to_vec())
            .await
            .expect("project writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("project creates");
        let _ = transport::read_frame(&mut client)
            .await
            .expect("project response");

        let import = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "import-prd-1".into(),
            client_id: "prd-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ImportPrd(
                generated::ImportPrd {
                    import_id: "import-1".into(),
                    project_id: "project-prd".into(),
                    origin: "prd.md".into(),
                    version: "v1".into(),
                    source_text: "# Plan\n\n## Task\n- [ ] Pass\n".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &import.encode_to_vec())
            .await
            .expect("import writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("import succeeds");
        let response = transport::read_frame(&mut client)
            .await
            .expect("import response");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "prd.imported");
        assert_eq!(
            journal
                .list_task_graph("project-prd")
                .await
                .unwrap()
                .0
                .len(),
            1
        );

        let mut duplicate = import;
        if let Some(generated::command_envelope::Command::ImportPrd(request)) =
            &mut duplicate.command
        {
            request.source_text.push_str("\n## Another");
            duplicate.request_id = "import-prd-2".into();
        }
        transport::write_frame(&mut client, &duplicate.encode_to_vec())
            .await
            .expect("duplicate writes");
        assert!(bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .is_err());
        assert_eq!(
            journal
                .list_task_graph("project-prd")
                .await
                .unwrap()
                .0
                .len(),
            1
        );
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn serves_run_doctor_with_real_storage_and_pipe_state() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-doctor-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "doctor-1".into(),
            client_id: "doctor-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::RunDoctor(
                generated::RunDoctor {
                    project_id: String::new(),
                    detail_level: 1,
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("doctor serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "doctor.report");
        let report: serde_json::Value =
            serde_json::from_slice(&event.payload).expect("doctor report is valid json");
        assert_eq!(report["bounded"], serde_json::json!(true));
        let checks = report["checks"].as_array().expect("checks array");
        assert_eq!(checks.len(), 7);
        let storage_check = checks
            .iter()
            .find(|check| check["id"] == "storage")
            .expect("storage check present");
        // A freshly-opened journal exists, is writable, and is on the
        // current schema version, so this reflects real (not fabricated)
        // storage state.
        assert_eq!(storage_check["status"], serde_json::json!("OK"));
        let permissions_check = checks
            .iter()
            .find(|check| check["id"] == "permissions")
            .expect("permissions check present");
        // No project_id was supplied, so the permissions probe is honestly
        // fail-closed rather than fabricated as healthy.
        assert_ne!(permissions_check["status"], serde_json::json!("OK"));
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn saves_and_lists_research_evidence_against_real_storage() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-research-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        let save = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "research-save-1".into(),
            client_id: "research-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SaveResearchEvidence(
                generated::SaveResearchEvidence {
                    work_item_id: "task-42".into(),
                    source_kind: "url".into(),
                    source_ref: "https://example.test/article".into(),
                    title: "Example Article".into(),
                    publisher: "Example Org".into(),
                    content_type: "text/html".into(),
                    raw_excerpt: "Useful finding sk-secret alice@example.test".into(),
                    retrieved_at_ms: 1_700_000_000_000,
                    ttl_ms: 3_600_000,
                },
            )),
        };
        transport::write_frame(&mut client, &save.encode_to_vec())
            .await
            .expect("save writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("save serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("save response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "research.evidence.saved");
        let saved: serde_json::Value =
            serde_json::from_slice(&event.payload).expect("save payload is valid json");
        assert_eq!(saved["work_item_id"], serde_json::json!("task-42"));
        let evidence_id = saved["id"].as_str().expect("id present").to_owned();
        assert_eq!(
            saved["evidence"]["excerpt"],
            serde_json::json!("Useful finding [REDACTED] [REDACTED]")
        );

        let list = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "research-list-1".into(),
            client_id: "research-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ListResearchEvidence(
                generated::ListResearchEvidence {
                    work_item_id: "task-42".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &list.encode_to_vec())
            .await
            .expect("list writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("list serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("list response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "research.evidence.list");
        let listed: serde_json::Value =
            serde_json::from_slice(&event.payload).expect("list payload is valid json");
        let records = listed["records"].as_array().expect("records array");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["id"], serde_json::json!(evidence_id));
        assert_eq!(records[0]["source_kind"], serde_json::json!("url"));
        assert_eq!(
            records[0]["redacted_excerpt"],
            serde_json::json!("Useful finding [REDACTED] [REDACTED]")
        );
        assert_eq!(records[0]["provenance_link"], serde_json::json!("task-42"));

        let _ = std::fs::remove_file(path);
    }

    fn run_research_fetch_command(
        work_item_id: &str,
        url: String,
        allowed_domains: Vec<String>,
        max_bytes: u64,
    ) -> generated::CommandEnvelope {
        generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: format!("research-fetch-{work_item_id}"),
            client_id: "research-fetch-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::RunResearchFetch(
                generated::RunResearchFetch {
                    work_item_id: work_item_id.into(),
                    url,
                    title: "Example Article".into(),
                    allowed_domains,
                    max_bytes,
                    max_latency_ms: 5_000,
                    max_cost_micros: 0,
                    ttl_ms: 3_600_000,
                },
            )),
        }
    }

    #[tokio::test]
    async fn run_research_fetch_persists_real_evidence_from_a_live_http_get() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/article"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string("Useful finding sk-secret alice@example.test")
                    .insert_header("content-type", "text/plain"),
            )
            .mount(&server)
            .await;
        let _private = evohime_tool_runtime::lock_private_override(Some(true));
        let domain = reqwest::Url::parse(&server.uri())
            .expect("mock uri parses")
            .host_str()
            .expect("mock uri has host")
            .to_ascii_lowercase();

        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-research-fetch-ok-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server_io) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server_io);

        let command = run_research_fetch_command(
            "task-fetch-ok",
            format!("{}/article", server.uri()),
            vec![domain],
            4096,
        );
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("fetch command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("fetch serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("fetch response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "research.fetch.completed");
        let payload: serde_json::Value =
            serde_json::from_slice(&event.payload).expect("fetch payload is valid json");
        assert_eq!(payload["state"], serde_json::json!("completed"));
        assert_eq!(
            payload["evidence"]["excerpt"],
            serde_json::json!("Useful finding [REDACTED] [REDACTED]")
        );
        let evidence_id = payload["id"].as_str().expect("id present").to_owned();

        let records = journal
            .list_research_evidence("task-fetch-ok")
            .await
            .expect("evidence lists from real storage");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, evidence_id);
        assert_eq!(
            records[0].redacted_excerpt,
            "Useful finding [REDACTED] [REDACTED]"
        );

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn run_research_fetch_denies_domain_outside_allowlist_and_persists_nothing() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/article"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("should not fetch"))
            .mount(&server)
            .await;
        let _private = evohime_tool_runtime::lock_private_override(Some(true));

        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-research-fetch-denied-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server_io) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server_io);

        let command = run_research_fetch_command(
            "task-fetch-denied",
            format!("{}/article", server.uri()),
            vec!["not-the-mock-domain.example".into()],
            4096,
        );
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("fetch command writes");
        let outcome = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            outcome.is_err(),
            "domain-denied fetch must fail the command"
        );
        assert_eq!(
            server
                .received_requests()
                .await
                .expect("requests tracked")
                .len(),
            0,
            "no network call should happen for a denied domain"
        );

        let records = journal
            .list_research_evidence("task-fetch-denied")
            .await
            .expect("list succeeds");
        assert!(records.is_empty(), "no evidence should be persisted");

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn run_research_fetch_blocks_ssrf_targets_and_persists_nothing() {
        let _private = evohime_tool_runtime::lock_private_override(Some(false));

        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-research-fetch-ssrf-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server_io) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server_io);

        let command = run_research_fetch_command(
            "task-fetch-ssrf",
            "http://127.0.0.1:9/".into(),
            vec!["127.0.0.1".into()],
            4096,
        );
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("fetch command writes");
        let outcome = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(outcome.is_err(), "ssrf-blocked fetch must fail the command");

        let records = journal
            .list_research_evidence("task-fetch-ssrf")
            .await
            .expect("list succeeds");
        assert!(records.is_empty(), "no evidence should be persisted");

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn run_research_fetch_rejects_oversized_response_and_persists_nothing() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/big"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("x".repeat(4_096)))
            .mount(&server)
            .await;
        let _private = evohime_tool_runtime::lock_private_override(Some(true));
        let domain = reqwest::Url::parse(&server.uri())
            .expect("mock uri parses")
            .host_str()
            .expect("mock uri has host")
            .to_ascii_lowercase();

        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-research-fetch-oversized-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server_io) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server_io);

        let command = run_research_fetch_command(
            "task-fetch-oversized",
            format!("{}/big", server.uri()),
            vec![domain],
            16,
        );
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("fetch command writes");
        let outcome = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(outcome.is_err(), "oversized response must fail the command");

        let records = journal
            .list_research_evidence("task-fetch-oversized")
            .await
            .expect("list succeeds");
        assert!(records.is_empty(), "no evidence should be persisted");

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn memory_create_list_search_archive_forget_round_trip_against_real_storage() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-memory-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "memory-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // Create two memories in the same task scope, one containing a
        // secret that must come back redacted.
        let create_one = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-create-1",
            generated::command_envelope::Command::CreateMemory(generated::CreateMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                title: "Rust build notes".into(),
                content: "Rust build cache lives in target/".into(),
                provenance_kind: "event".into(),
                provenance_id: "evt-1".into(),
                provenance_locator: String::new(),
                privacy: "internal".into(),
                ttl_ms: 3_600_000,
            }),
        )
        .await;
        assert_eq!(create_one.event_type, "memory.created");
        let created_one: serde_json::Value =
            serde_json::from_slice(&create_one.payload).expect("create payload is valid json");
        let memory_one_id = created_one["record"]["id"]
            .as_str()
            .expect("id present")
            .to_owned();

        let create_two = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-create-2",
            generated::command_envelope::Command::CreateMemory(generated::CreateMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                title: "Deployment secret".into(),
                content: "Token is sk-secret, keep it safe".into(),
                provenance_kind: "event".into(),
                provenance_id: "evt-2".into(),
                provenance_locator: String::new(),
                privacy: "internal".into(),
                ttl_ms: 3_600_000,
            }),
        )
        .await;
        assert_eq!(create_two.event_type, "memory.created");
        let created_two: serde_json::Value =
            serde_json::from_slice(&create_two.payload).expect("create payload is valid json");
        assert_eq!(
            created_two["record"]["content"],
            serde_json::json!("Token is [REDACTED] keep it safe")
        );
        let memory_two_id = created_two["record"]["id"]
            .as_str()
            .expect("id present")
            .to_owned();

        // List returns both, newest first.
        let list = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-list-1",
            generated::command_envelope::Command::ListMemory(generated::ListMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                include_archived: false,
                limit: 10,
            }),
        )
        .await;
        assert_eq!(list.event_type, "memory.list");
        let listed: serde_json::Value =
            serde_json::from_slice(&list.payload).expect("list payload is valid json");
        let records = listed["records"].as_array().expect("records array");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["id"], serde_json::json!(memory_two_id));
        assert_eq!(records[1]["id"], serde_json::json!(memory_one_id));
        assert_eq!(records[0]["project_id"], serde_json::json!("proj-1"));
        assert_eq!(records[0]["secondary_id"], serde_json::json!("task-1"));
        // ListMemory is metadata-only: no statement, no provenance body.
        for record in records {
            assert!(
                record.get("statement").is_none(),
                "list must not carry body"
            );
            assert!(
                record.get("provenance").is_none(),
                "list must not carry provenance body"
            );
            assert_eq!(record["confirmation_state"], serde_json::json!("confirmed"));
            assert_eq!(record["kind"], serde_json::json!("entity"));
        }

        // The body is reachable only through an explicit GetMemory.
        let fetched = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-get-1",
            generated::command_envelope::Command::GetMemory(generated::GetMemory {
                id: memory_one_id.clone(),
            }),
        )
        .await;
        assert_eq!(fetched.event_type, "memory.record");
        let body: serde_json::Value =
            serde_json::from_slice(&fetched.payload).expect("get payload is valid json");
        assert_eq!(body["record"]["body_redacted"], serde_json::json!(false));
        assert_eq!(
            body["record"]["statement"],
            serde_json::json!("Rust build cache lives in target/")
        );
        assert_eq!(
            body["supersession_chain"],
            serde_json::json!([memory_one_id])
        );

        // Search only matches the record with "rust" in it.
        let search = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-search-1",
            generated::command_envelope::Command::SearchMemory(generated::SearchMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                query: "rust".into(),
                limit: 10,
            }),
        )
        .await;
        assert_eq!(search.event_type, "memory.search");
        let searched: serde_json::Value =
            serde_json::from_slice(&search.payload).expect("search payload is valid json");
        let hits = searched["records"].as_array().expect("records array");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["id"], serde_json::json!(memory_one_id));

        // Archive without an approval token is rejected.
        let archive_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "memory-archive-denied".into(),
            client_id: "memory-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ArchiveMemory(
                generated::ArchiveMemory {
                    id: memory_one_id.clone(),
                    approval_id: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &archive_envelope.encode_to_vec())
            .await
            .expect("archive request writes");
        let denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(denied.is_err(), "archive without approval must fail");

        // Archive with an approval token succeeds and is audited.
        let archive = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-archive-1",
            generated::command_envelope::Command::ArchiveMemory(generated::ArchiveMemory {
                id: memory_one_id.clone(),
                approval_id: "approval-1".into(),
            }),
        )
        .await;
        assert_eq!(archive.event_type, "memory.archived");
        let archived: serde_json::Value =
            serde_json::from_slice(&archive.payload).expect("archive payload is valid json");
        assert_eq!(archived["archived"], serde_json::json!(true));

        // Archived record is hidden from default listing.
        let list_after_archive = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-list-2",
            generated::command_envelope::Command::ListMemory(generated::ListMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                include_archived: false,
                limit: 10,
            }),
        )
        .await;
        let listed_after: serde_json::Value = serde_json::from_slice(&list_after_archive.payload)
            .expect("list payload is valid json");
        let records_after = listed_after["records"].as_array().expect("records array");
        assert_eq!(records_after.len(), 1);
        assert_eq!(records_after[0]["id"], serde_json::json!(memory_two_id));

        // Forget with an approval token erases title/content.
        let forget = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-forget-1",
            generated::command_envelope::Command::ForgetMemory(generated::ForgetMemory {
                id: memory_two_id.clone(),
                approval_id: "approval-2".into(),
            }),
        )
        .await;
        assert_eq!(forget.event_type, "memory.forgotten");
        let forgotten: serde_json::Value =
            serde_json::from_slice(&forget.payload).expect("forget payload is valid json");
        assert_eq!(forgotten["forgotten"], serde_json::json!(true));

        let list_after_forget = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-list-3",
            generated::command_envelope::Command::ListMemory(generated::ListMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                include_archived: true,
                limit: 10,
            }),
        )
        .await;
        let listed_final: serde_json::Value =
            serde_json::from_slice(&list_after_forget.payload).expect("list payload is valid json");
        let records_final = listed_final["records"].as_array().expect("records array");
        // Forgotten records are excluded even with include_archived=true.
        assert!(records_final
            .iter()
            .all(|record| record["id"] != serde_json::json!(memory_two_id)));
        assert_eq!(forgotten["forgotten"], serde_json::json!(true));
        assert!(
            forgotten["tombstone_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty()),
            "forget must produce a tombstone id"
        );

        // A forgotten record still answers GetMemory, but only with metadata.
        let forgotten_body = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-get-forgotten",
            generated::command_envelope::Command::GetMemory(generated::GetMemory {
                id: memory_two_id.clone(),
            }),
        )
        .await;
        let forgotten_json: serde_json::Value =
            serde_json::from_slice(&forgotten_body.payload).expect("payload is valid json");
        assert_eq!(
            forgotten_json["record"]["body_redacted"],
            serde_json::json!(true)
        );
        assert!(forgotten_json["record"].get("statement").is_none());

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn memory_pending_conflict_confirm_reject_supersede_round_trip() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-memory-pending-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "memory-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // Seed the store directly: extraction candidates are produced by
        // Core's policy gate, not by an IPC caller, so the IPC surface only
        // has to prove that pending records can be reviewed and resolved.
        let seed = |id: &str, state: &str, statement: &str| {
            let mut record = evohime_local_storage::memory_store::MemoryRecord::new(
                evohime_local_storage::memory_store::MemoryRecordInput {
                    id: id.to_owned(),
                    scope: evohime_local_storage::memory_store::MemoryScope::Project,
                    scope_id: "proj-1".to_owned(),
                    title: "Язык интерфейса".to_owned(),
                    content: statement.to_owned(),
                    provenance: "{\"message_id\":\"msg-1\"}".to_owned(),
                    privacy: evohime_local_storage::memory_store::MemoryPrivacy::Internal,
                    created_at: "1000".to_owned(),
                    expires_at: Some("99999999999999".to_owned()),
                },
            )
            .expect("record builds");
            record.extraction = evohime_local_storage::memory_store::MemoryExtractionFields {
                kind: "preference".to_owned(),
                canonical_subject: Some("язык интерфейса".to_owned()),
                confirmation_state: state.to_owned(),
                model_confidence: 0.9,
                verification_confidence: 0.0,
                extractor_version: "extractor-v1".to_owned(),
                policy_version: "extraction-policy-v1".to_owned(),
                ..Default::default()
            };
            record
        };
        journal
            .save_memory(&seed("active-1", "confirmed", "UI на русском языке"))
            .await
            .expect("active memory saves");
        journal
            .save_memory(&seed(
                "pending-1",
                "pending_confirmation",
                "UI на английском языке",
            ))
            .await
            .expect("pending memory saves");
        journal
            .save_memory(&seed(
                "pending-2",
                "pending_confirmation",
                "UI на русском языке",
            ))
            .await
            .expect("duplicate pending saves");

        let pending = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-pending-1",
            generated::command_envelope::Command::ListMemoryPending(generated::ListMemoryPending {
                scope_kind: "project".into(),
                project_id: "proj-1".into(),
                secondary_id: String::new(),
                limit: 10,
                workspace_path: String::new(),
            }),
        )
        .await;
        assert_eq!(pending.event_type, "memory.pending");
        let pending_json: serde_json::Value =
            serde_json::from_slice(&pending.payload).expect("pending payload is valid json");
        assert_eq!(
            pending_json["counts"]["pending_confirmation"],
            serde_json::json!(2)
        );
        assert_eq!(pending_json["counts"]["confirmed"], serde_json::json!(1));
        for record in pending_json["records"].as_array().expect("records array") {
            assert!(
                record.get("statement").is_none(),
                "queue must stay metadata-only"
            );
        }

        // Only the incompatible statement is a conflict; the duplicate is not.
        let conflicts = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-conflicts-1",
            generated::command_envelope::Command::GetMemoryConflicts(
                generated::GetMemoryConflicts {
                    scope_kind: "project".into(),
                    project_id: "proj-1".into(),
                    secondary_id: String::new(),
                    limit: 10,
                    workspace_path: String::new(),
                },
            ),
        )
        .await;
        assert_eq!(conflicts.event_type, "memory.conflicts");
        let conflicts_json: serde_json::Value =
            serde_json::from_slice(&conflicts.payload).expect("conflicts payload is valid json");
        let conflict_list = conflicts_json["conflicts"].as_array().expect("conflicts");
        assert_eq!(conflict_list.len(), 1);
        assert_eq!(
            conflict_list[0]["pending"]["id"],
            serde_json::json!("pending-1")
        );
        assert_eq!(
            conflict_list[0]["active"]["id"],
            serde_json::json!("active-1")
        );

        // "Изменить": the user rewrites the statement before deciding. The
        // record becomes a user assertion but stays pending.
        let revised = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-revise-1",
            generated::command_envelope::Command::ReviseMemoryCandidate(
                generated::ReviseMemoryCandidate {
                    id: "pending-1".into(),
                    statement: "UI строго на английском языке".into(),
                    session_only: false,
                    session_id: String::new(),
                    approval_id: "approval-revise".into(),
                    idempotency_key: "key-revise".into(),
                },
            ),
        )
        .await;
        assert_eq!(revised.event_type, "memory.revised");
        let revised_json: serde_json::Value =
            serde_json::from_slice(&revised.payload).expect("payload is valid json");
        assert_eq!(
            revised_json["record"]["confirmation_state"],
            serde_json::json!("pending_confirmation")
        );
        assert_eq!(
            revised_json["record"]["source_trust"],
            serde_json::json!("user")
        );
        assert_eq!(
            revised_json["record"]["extractor_version"],
            serde_json::json!("user_edited")
        );
        // Even the revision response stays metadata-only.
        assert!(revised_json["record"].get("statement").is_none());

        // "Только на эту сессию": no persistent memory survives.
        journal
            .save_memory(&seed(
                "pending-3",
                "pending_confirmation",
                "временное правило",
            ))
            .await
            .expect("third pending saves");
        let session_only = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-session-only-1",
            generated::command_envelope::Command::ReviseMemoryCandidate(
                generated::ReviseMemoryCandidate {
                    id: "pending-3".into(),
                    statement: String::new(),
                    session_only: true,
                    session_id: "session-1".into(),
                    approval_id: "approval-session".into(),
                    idempotency_key: "key-session".into(),
                },
            ),
        )
        .await;
        let session_json: serde_json::Value =
            serde_json::from_slice(&session_only.payload).expect("payload is valid json");
        assert_eq!(session_json["session_only"], serde_json::json!(true));
        assert_eq!(session_json["state"], serde_json::json!("rejected"));
        let notes = journal
            .list_memory_session_notes("session-1", &0.to_string())
            .await
            .expect("session notes read");
        assert_eq!(notes.len(), 1, "the statement lives only as a session note");

        // A session-only note without a session id is refused.
        let no_session = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "memory-session-only-bad".into(),
            client_id: "memory-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ReviseMemoryCandidate(
                generated::ReviseMemoryCandidate {
                    id: "pending-2".into(),
                    statement: String::new(),
                    session_only: true,
                    session_id: String::new(),
                    approval_id: "approval-session-2".into(),
                    idempotency_key: "key-session-2".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &no_session.encode_to_vec())
            .await
            .expect("request writes");
        assert!(
            bridge
                .process_once(&mut server_reader, &mut server_writer)
                .await
                .is_err(),
            "a session-only note needs a session id"
        );

        // Confirm without approval is rejected.
        let denied_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "memory-confirm-denied".into(),
            client_id: "memory-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ConfirmMemory(
                generated::ConfirmMemory {
                    ids: vec!["pending-1".into()],
                    approval_id: String::new(),
                    idempotency_key: "key-1".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &denied_envelope.encode_to_vec())
            .await
            .expect("denied request writes");
        assert!(
            bridge
                .process_once(&mut server_reader, &mut server_writer)
                .await
                .is_err(),
            "confirm without approval must fail"
        );

        // Confirm without an idempotency key is rejected too.
        let no_key = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "memory-confirm-no-key".into(),
            client_id: "memory-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ConfirmMemory(
                generated::ConfirmMemory {
                    ids: vec!["pending-1".into()],
                    approval_id: "approval-1".into(),
                    idempotency_key: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &no_key.encode_to_vec())
            .await
            .expect("request writes");
        assert!(
            bridge
                .process_once(&mut server_reader, &mut server_writer)
                .await
                .is_err(),
            "confirm without an idempotency key must fail"
        );

        // Approved confirm applies, and repeating it is safe.
        for request_id in ["memory-confirm-1", "memory-confirm-1-replay"] {
            let confirmed = send(
                &bridge,
                &mut client,
                &mut server_reader,
                &mut server_writer,
                request_id,
                generated::command_envelope::Command::ConfirmMemory(generated::ConfirmMemory {
                    ids: vec!["pending-1".into()],
                    approval_id: "approval-1".into(),
                    idempotency_key: "key-1".into(),
                }),
            )
            .await;
            assert_eq!(confirmed.event_type, "memory.confirmed");
            let json: serde_json::Value =
                serde_json::from_slice(&confirmed.payload).expect("payload is valid json");
            assert_eq!(json["results"][0]["state"], serde_json::json!("confirmed"));
        }

        // Batch reject is terminal: a later confirm reports the real state.
        let rejected = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-reject-1",
            generated::command_envelope::Command::RejectMemory(generated::RejectMemory {
                ids: vec!["pending-2".into()],
                approval_id: "approval-2".into(),
                idempotency_key: "key-2".into(),
            }),
        )
        .await;
        assert_eq!(rejected.event_type, "memory.rejected");
        let rejected_json: serde_json::Value =
            serde_json::from_slice(&rejected.payload).expect("payload is valid json");
        assert_eq!(
            rejected_json["results"][0]["state"],
            serde_json::json!("rejected")
        );

        let reopen = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-confirm-2",
            generated::command_envelope::Command::ConfirmMemory(generated::ConfirmMemory {
                ids: vec!["pending-2".into()],
                approval_id: "approval-3".into(),
                idempotency_key: "key-3".into(),
            }),
        )
        .await;
        let reopen_json: serde_json::Value =
            serde_json::from_slice(&reopen.payload).expect("payload is valid json");
        assert_eq!(
            reopen_json["results"][0]["state"],
            serde_json::json!("rejected")
        );
        assert_eq!(
            reopen_json["results"][0]["applied"],
            serde_json::json!(false)
        );

        // The conflict is resolved only by an explicit supersede.
        let superseded = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-supersede-1",
            generated::command_envelope::Command::SupersedeMemory(generated::SupersedeMemory {
                old_id: "active-1".into(),
                new_id: "pending-1".into(),
                reason: "user_choice".into(),
                approval_id: "approval-4".into(),
                idempotency_key: "key-4".into(),
            }),
        )
        .await;
        assert_eq!(superseded.event_type, "memory.superseded");
        let superseded_json: serde_json::Value =
            serde_json::from_slice(&superseded.payload).expect("payload is valid json");
        assert_eq!(
            superseded_json["supersession_chain"],
            serde_json::json!(["active-1", "pending-1"])
        );

        // An unsupported reason is refused rather than stored as free text.
        let bad_reason = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "memory-supersede-bad".into(),
            client_id: "memory-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SupersedeMemory(
                generated::SupersedeMemory {
                    old_id: "pending-1".into(),
                    new_id: "pending-2".into(),
                    reason: "because".into(),
                    approval_id: "approval-5".into(),
                    idempotency_key: "key-5".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &bad_reason.encode_to_vec())
            .await
            .expect("request writes");
        assert!(
            bridge
                .process_once(&mut server_reader, &mut server_writer)
                .await
                .is_err(),
            "an unsupported supersession reason must fail"
        );

        // After resolution only the winning record is retrievable.
        let search = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-search-final",
            generated::command_envelope::Command::SearchMemory(generated::SearchMemory {
                scope_kind: "project".into(),
                project_id: "proj-1".into(),
                secondary_id: String::new(),
                query: "ui".into(),
                limit: 10,
            }),
        )
        .await;
        let search_json: serde_json::Value =
            serde_json::from_slice(&search.payload).expect("payload is valid json");
        let hits = search_json["records"].as_array().expect("records array");
        assert_eq!(
            hits.iter()
                .map(|hit| hit["id"].as_str().unwrap_or_default())
                .collect::<Vec<_>>(),
            ["pending-1"]
        );

        let _ = std::fs::remove_file(path);
    }

    fn capability_manifest_json(name: &str, version: &str, risk_class: &str) -> String {
        let content_hash = "0123456789abcdef0123456789abcdef";
        let signature =
            crate::capability_registry::test_sign_with_trusted_key(name, version, content_hash);
        serde_json::json!({
            "name": name,
            "version": version,
            "content_hash": content_hash,
            "signature": signature,
            "signing_key_id": "evohime-dev-1",
            "roles": [{
                "name": "reviewer",
                "version": "1",
                "content_hash": "abcdef0123456789abcdef0123456789"
            }],
            "skills": [],
            "allowed_tools": ["filesystem.read", "git.diff"],
            "allowed_domains": ["docs.example.com"],
            "protected_paths": ["src"],
            "risk_class": risk_class,
            "install": {
                "source": "local_archive",
                "allow_install_scripts": false,
                "allow_update": true,
                "rollback_on_failure": true
            }
        })
        .to_string()
    }

    #[tokio::test]
    async fn capability_install_list_match_remove_round_trip_against_real_storage() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-capability-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "capability-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // HTTPS installation requires a real URL and a trusted hash. A
        // request without those inputs must still be rejected before storage.
        let https_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-install-https".into(),
            client_id: "capability-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::InstallCapability(
                generated::InstallCapability {
                    manifest_json: capability_manifest_json("reviewer", "1.0.0", "medium"),
                    install_source: "https_archive".into(),
                    source_path: String::new(),
                    expected_content_hash: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &https_envelope.encode_to_vec())
            .await
            .expect("https install request writes");
        let https_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            https_denied.is_err(),
            "https_archive install source must be rejected in this pass"
        );

        // Installing a manifest with a malformed content_hash must be
        // rejected via the real RegistryError::InvalidHash path.
        let bad_hash_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-install-bad-hash".into(),
            client_id: "capability-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::InstallCapability(
                generated::InstallCapability {
                    manifest_json: capability_manifest_json("bad-hash", "1.0.0", "medium")
                        .replace("0123456789abcdef0123456789abcdef", "not-a-hex-hash"),
                    install_source: "local_archive".into(),
                    source_path: String::new(),
                    expected_content_hash: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &bad_hash_envelope.encode_to_vec())
            .await
            .expect("bad hash install request writes");
        let bad_hash_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            bad_hash_denied.is_err(),
            "manifest with a malformed content_hash must be rejected"
        );

        // Installing a manifest with an invalid risk_class must be rejected
        // before it ever reaches storage.
        let bad_risk_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-install-bad-risk".into(),
            client_id: "capability-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::InstallCapability(
                generated::InstallCapability {
                    manifest_json: capability_manifest_json("bad-risk", "1.0.0", "extreme"),
                    install_source: "local_archive".into(),
                    source_path: String::new(),
                    expected_content_hash: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &bad_risk_envelope.encode_to_vec())
            .await
            .expect("bad risk install request writes");
        let bad_risk_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            bad_risk_denied.is_err(),
            "manifest with an invalid risk_class must be rejected"
        );

        // A valid local-archive manifest installs successfully.
        let install = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-install-1",
            generated::command_envelope::Command::InstallCapability(generated::InstallCapability {
                manifest_json: capability_manifest_json("reviewer", "1.0.0", "medium"),
                install_source: "local_archive".into(),
                source_path: "C:/archives/reviewer.zip".into(),
                expected_content_hash: String::new(),
            }),
        )
        .await;
        assert_eq!(install.event_type, "capability.installed");
        let installed: serde_json::Value =
            serde_json::from_slice(&install.payload).expect("install payload is valid json");
        assert_eq!(installed["manifest"]["name"], serde_json::json!("reviewer"));

        // List returns the installed manifest.
        let list = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-list-1",
            generated::command_envelope::Command::ListCapabilities(generated::ListCapabilities {
                limit: 10,
            }),
        )
        .await;
        assert_eq!(list.event_type, "capability.list");
        let listed: serde_json::Value =
            serde_json::from_slice(&list.payload).expect("list payload is valid json");
        let manifests = listed["manifests"].as_array().expect("manifests array");
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0]["name"], serde_json::json!("reviewer"));

        // Match selects the installed manifest for a fitting query.
        let matched = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-match-1",
            generated::command_envelope::Command::MatchCapabilities(generated::MatchCapabilities {
                intent: "review reviewer".into(),
                required_tools: vec!["git.diff".into()],
                required_domains: vec!["docs.example.com".into()],
                requested_risk: "low".into(),
            }),
        )
        .await;
        assert_eq!(matched.event_type, "capability.match");
        let matches: serde_json::Value =
            serde_json::from_slice(&matched.payload).expect("match payload is valid json");
        let hits = matches["matches"].as_array().expect("matches array");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["manifest_name"], serde_json::json!("reviewer"));

        // Remove deletes the manifest.
        let removed = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-remove-1",
            generated::command_envelope::Command::RemoveCapability(generated::RemoveCapability {
                id: "reviewer".into(),
            }),
        )
        .await;
        assert_eq!(removed.event_type, "capability.removed");
        let removed_payload: serde_json::Value =
            serde_json::from_slice(&removed.payload).expect("remove payload is valid json");
        assert_eq!(removed_payload["removed"], serde_json::json!(true));

        // Removing again is rejected: the manifest is already gone.
        let remove_again_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-remove-2".into(),
            client_id: "capability-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::RemoveCapability(
                generated::RemoveCapability {
                    id: "reviewer".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &remove_again_envelope.encode_to_vec())
            .await
            .expect("second remove request writes");
        let remove_again = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            remove_again.is_err(),
            "removing a manifest that no longer exists must fail"
        );

        let list_after_remove = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-list-2",
            generated::command_envelope::Command::ListCapabilities(generated::ListCapabilities {
                limit: 10,
            }),
        )
        .await;
        let listed_after: serde_json::Value =
            serde_json::from_slice(&list_after_remove.payload).expect("list payload is valid json");
        assert!(listed_after["manifests"]
            .as_array()
            .expect("manifests array")
            .is_empty());

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn capability_selection_get_pin_replace_round_trip_against_real_storage() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-capability-selection-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "capability-selection-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // Install two candidate manifests so replace() has a real
        // alternative to switch to.
        for name in ["reviewer", "planner"] {
            let install = send(
                &bridge,
                &mut client,
                &mut server_reader,
                &mut server_writer,
                &format!("capability-selection-install-{name}"),
                generated::command_envelope::Command::InstallCapability(
                    generated::InstallCapability {
                        manifest_json: capability_manifest_json(name, "1.0.0", "medium"),
                        install_source: "local_archive".into(),
                        source_path: format!("C:/archives/{name}.zip"),
                        expected_content_hash: String::new(),
                    },
                ),
            )
            .await;
            assert_eq!(install.event_type, "capability.installed");
        }

        let query_fields = || generated::GetCapabilitySelection {
            task_id: "task-1".into(),
            intent: "review reviewer".into(),
            required_tools: vec!["git.diff".into()],
            required_domains: vec!["docs.example.com".into()],
            requested_risk: "low".into(),
        };

        // First GetCapabilitySelection: no prior state, so the matcher's
        // top-scoring manifest is auto-selected and persisted.
        let selected = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-selection-get-1",
            generated::command_envelope::Command::GetCapabilitySelection(query_fields()),
        )
        .await;
        assert_eq!(selected.event_type, "capability.selection");
        let selected_json: serde_json::Value =
            serde_json::from_slice(&selected.payload).expect("selection payload is valid json");
        assert_eq!(
            selected_json["selection"]["manifest_name"],
            serde_json::json!("reviewer")
        );
        assert_eq!(selected_json["origin"], serde_json::json!("auto"));

        // Pinning persists origin=pinned for the same task_id.
        let pinned = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-selection-pin-1",
            generated::command_envelope::Command::PinCapabilitySelection(
                generated::PinCapabilitySelection {
                    task_id: "task-1".into(),
                },
            ),
        )
        .await;
        assert_eq!(pinned.event_type, "capability.selection.pinned");
        let pinned_json: serde_json::Value =
            serde_json::from_slice(&pinned.payload).expect("pin payload is valid json");
        assert_eq!(pinned_json["origin"], serde_json::json!("pinned"));
        assert!(pinned_json["selection"]["pinned"].as_bool().unwrap());

        // A subsequent GetCapabilitySelection must not silently override the
        // pin, even though the matcher would still pick "reviewer" here --
        // the persisted origin stays "pinned".
        let reconciled = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-selection-get-2",
            generated::command_envelope::Command::GetCapabilitySelection(query_fields()),
        )
        .await;
        let reconciled_json: serde_json::Value =
            serde_json::from_slice(&reconciled.payload).expect("selection payload is valid json");
        assert_eq!(reconciled_json["origin"], serde_json::json!("pinned"));

        // Explicitly replacing switches the persisted selection to
        // "planner" and marks origin=replaced.
        let replaced = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-selection-replace-1",
            generated::command_envelope::Command::ReplaceCapabilitySelection(
                generated::ReplaceCapabilitySelection {
                    task_id: "task-1".into(),
                    manifest_name: "planner".into(),
                    intent: "review reviewer".into(),
                    required_tools: vec!["git.diff".into()],
                    required_domains: vec!["docs.example.com".into()],
                    requested_risk: "low".into(),
                },
            ),
        )
        .await;
        assert_eq!(replaced.event_type, "capability.selection.replaced");
        let replaced_json: serde_json::Value =
            serde_json::from_slice(&replaced.payload).expect("replace payload is valid json");
        assert_eq!(
            replaced_json["selection"]["manifest_name"],
            serde_json::json!("planner")
        );
        assert_eq!(replaced_json["origin"], serde_json::json!("replaced"));

        // A fresh GetCapabilitySelection still returns the replaced choice
        // -- proving persistence survives a new request (simulated
        // reconnect), matching the store's own round-trip contract.
        let after_replace = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-selection-get-3",
            generated::command_envelope::Command::GetCapabilitySelection(query_fields()),
        )
        .await;
        let after_replace_json: serde_json::Value = serde_json::from_slice(&after_replace.payload)
            .expect("selection payload is valid json");
        assert_eq!(
            after_replace_json["selection"]["manifest_name"],
            serde_json::json!("planner")
        );
        assert_eq!(after_replace_json["origin"], serde_json::json!("replaced"));

        // Pinning for a task_id with no persisted selection must fail.
        let pin_missing_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-selection-pin-missing".into(),
            client_id: "capability-selection-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(
                generated::command_envelope::Command::PinCapabilitySelection(
                    generated::PinCapabilitySelection {
                        task_id: "task-never-selected".into(),
                    },
                ),
            ),
        };
        transport::write_frame(&mut client, &pin_missing_envelope.encode_to_vec())
            .await
            .expect("pin-missing request writes");
        let pin_missing = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            pin_missing.is_err(),
            "pinning a task with no persisted selection must fail"
        );

        let _ = std::fs::remove_file(path);
    }

    /// Proves the read-only child delegation boundary holds end-to-end
    /// through the real IPC command path, not just at the pure-function
    /// level (`child_runtime::ChildTaskRequest::validate` /
    /// `child_runtime::accept_report` unit tests): a request naming a
    /// non-read-only capability is rejected, a nested-child request is
    /// rejected, a report with secret-like content is rejected, and a
    /// valid read-only request plus matching valid report round-trips
    /// through save -> submit -> list successfully. This test does not
    /// spawn or execute any child agent; it only proves the
    /// request/report validation and persistence boundary.
    #[tokio::test]
    async fn child_handoff_request_report_security_boundary_against_real_storage() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-child-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "child-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // (a) A request naming a non-read-only capability (workspace.write)
        // must be rejected end-to-end, not just by the pure function.
        let write_capability_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "child-request-write-capability".into(),
            client_id: "child-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SubmitChildRequest(
                generated::SubmitChildRequest {
                    child_task_id: "child-write".into(),
                    parent_task_id: "task-1".into(),
                    role: "researcher".into(),
                    kind: "code_search".into(),
                    reduced_context: vec!["inspect src".into()],
                    max_output_bytes: 4096,
                    requested_capabilities: vec!["workspace.write".into()],
                    parent_is_child: false,
                },
            )),
        };
        transport::write_frame(&mut client, &write_capability_envelope.encode_to_vec())
            .await
            .expect("write-capability request writes");
        let write_capability_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            write_capability_denied.is_err(),
            "a request naming a non-read-only capability must be rejected"
        );

        // (b) A nested child (parent_is_child = true) must be rejected.
        let nested_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "child-request-nested".into(),
            client_id: "child-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SubmitChildRequest(
                generated::SubmitChildRequest {
                    child_task_id: "child-nested".into(),
                    parent_task_id: "task-1".into(),
                    role: "researcher".into(),
                    kind: "code_search".into(),
                    reduced_context: vec!["inspect src".into()],
                    max_output_bytes: 4096,
                    requested_capabilities: vec!["workspace.read".into()],
                    parent_is_child: true,
                },
            )),
        };
        transport::write_frame(&mut client, &nested_envelope.encode_to_vec())
            .await
            .expect("nested request writes");
        let nested_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            nested_denied.is_err(),
            "a nested child request (parent_is_child = true) must be rejected"
        );

        // A valid read-only request submits successfully.
        let submitted = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "child-request-valid",
            generated::command_envelope::Command::SubmitChildRequest(
                generated::SubmitChildRequest {
                    child_task_id: "child-1".into(),
                    parent_task_id: "task-1".into(),
                    role: "researcher".into(),
                    kind: "code_search".into(),
                    reduced_context: vec!["inspect src".into()],
                    max_output_bytes: 4096,
                    requested_capabilities: vec!["workspace.read".into(), "git.diff".into()],
                    parent_is_child: false,
                },
            ),
        )
        .await;
        assert_eq!(submitted.event_type, "child.request.submitted");
        let submitted_payload: serde_json::Value =
            serde_json::from_slice(&submitted.payload).expect("submit payload is valid json");
        assert_eq!(
            submitted_payload["request"]["child_task_id"],
            serde_json::json!("child-1")
        );

        // (c) A report containing secret-like content must be rejected,
        // even though it matches a valid, already-persisted request.
        let secret_report_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "child-report-secret".into(),
            client_id: "child-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SubmitChildReport(
                generated::SubmitChildReport {
                    child_task_id: "child-1".into(),
                    status: "complete".into(),
                    summary: "api_key=do-not-leak".into(),
                    findings: vec!["module is bounded".into()],
                    sources: vec!["src/lib.rs:10".into()],
                    confidence_percent: 90,
                },
            )),
        };
        transport::write_frame(&mut client, &secret_report_envelope.encode_to_vec())
            .await
            .expect("secret report writes");
        let secret_report_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            secret_report_denied.is_err(),
            "a report containing secret-like content must be rejected"
        );

        // (d) A matching, valid report round-trips through
        // save -> submit -> list successfully.
        let accepted = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "child-report-valid",
            generated::command_envelope::Command::SubmitChildReport(generated::SubmitChildReport {
                child_task_id: "child-1".into(),
                status: "complete".into(),
                summary: "found one relevant module".into(),
                findings: vec!["module is bounded".into()],
                sources: vec!["src/lib.rs:10".into()],
                confidence_percent: 90,
            }),
        )
        .await;
        assert_eq!(accepted.event_type, "child.report.accepted");
        let accepted_payload: serde_json::Value =
            serde_json::from_slice(&accepted.payload).expect("report payload is valid json");
        assert_eq!(
            accepted_payload["report"]["child_task_id"],
            serde_json::json!("child-1")
        );

        // A separately requested handoff persists and lists back through
        // the real command path too.
        let handoff = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "child-handoff-valid",
            generated::command_envelope::Command::RequestChildHandoff(
                generated::RequestChildHandoff {
                    handoff_id: "handoff-1".into(),
                    task_id: "task-1".into(),
                    kind: "delegate".into(),
                    from_role: "coordinator".into(),
                    from_name: String::new(),
                    to_role: "researcher".into(),
                    to_name: String::new(),
                    purpose: "investigate module bounds".into(),
                    payload: std::collections::HashMap::new(),
                    sequence: 1,
                },
            ),
        )
        .await;
        assert_eq!(handoff.event_type, "child.handoff.requested");

        let listed_handoffs = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "child-handoff-list",
            generated::command_envelope::Command::ListChildHandoffs(generated::ListChildHandoffs {
                task_id: "task-1".into(),
                limit: 10,
            }),
        )
        .await;
        assert_eq!(listed_handoffs.event_type, "child.handoff.list");
        let listed_handoffs_payload: serde_json::Value =
            serde_json::from_slice(&listed_handoffs.payload)
                .expect("handoff list payload is valid json");
        let handoffs = listed_handoffs_payload["handoffs"]
            .as_array()
            .expect("handoffs array");
        assert_eq!(handoffs.len(), 1);
        assert_eq!(handoffs[0]["handoff_id"], serde_json::json!("handoff-1"));

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn list_verify_and_export_receipts_over_ipc() {
        let data_root =
            std::env::temp_dir().join(format!("evohime-ipc-receipts-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data_root);
        std::fs::create_dir_all(&data_root).expect("data root");
        let journal_path = data_root.join("events.db");
        let keys = ReceiptKeyManager::new(&data_root);
        keys.initialize().expect("keys initialize");
        let journal = EventJournal::open(&journal_path).expect("journal opens");
        {
            let mut database = journal.database().lock().await;
            let signer = crate::CoreReceiptSigner(Arc::new(ReceiptKeyManager::new(&data_root)));
            let mut runtime =
                evohime_receipts::runtime::ReceiptRuntime::new(database.connection_mut(), &signer)
                    .unwrap();
            let action_id = uuid::Uuid::now_v7();
            let request = evohime_receipts::runtime::ActionRequest {
                action_id,
                task_id: "receipts-task".into(),
                run_id: "receipts-run".into(),
                tool_name: "filesystem.read".into(),
                policy_id: "permission:FilesystemRead".into(),
                normalized_scope: "workspace".into(),
                input: serde_json::json!({"path":"a.txt"}),
                policy_decision: evohime_receipts::runtime::PolicyDecision::Allow,
                approval_id: None,
                parent_approval_ref: None,
                preview: "read a.txt".into(),
            };
            runtime.prepare(request.clone()).unwrap();
            runtime.mark_started(action_id).unwrap();
            runtime
                .complete(&request, "succeeded", &"a".repeat(64), None)
                .unwrap();
        }
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(64 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "receipts-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        let listed = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "list-1",
            generated::command_envelope::Command::ListReceipts(generated::ListReceipts {
                task_id: "receipts-task".into(),
                ..Default::default()
            }),
        )
        .await;
        assert_eq!(listed.event_type, "receipts.listed");
        let listed_payload: serde_json::Value = serde_json::from_slice(&listed.payload).unwrap();
        assert_eq!(listed_payload["ok"], true);
        assert_eq!(listed_payload["rows"].as_array().unwrap().len(), 2);

        let verified = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "verify-1",
            generated::command_envelope::Command::VerifyReceipts(generated::VerifyReceipts {
                task_id: "receipts-task".into(),
                ..Default::default()
            }),
        )
        .await;
        assert_eq!(verified.event_type, "receipts.verified");
        let verified_payload: serde_json::Value =
            serde_json::from_slice(&verified.payload).unwrap();
        assert_eq!(verified_payload["ok"], true);
        assert_eq!(verified_payload["status"], "verified");
        assert_eq!(verified_payload["actual_verified_count"], 2);

        let destination = data_root.join("export-bundle");
        let exported = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "export-1",
            generated::command_envelope::Command::ExportReceipts(generated::ExportReceipts {
                destination_path: destination.display().to_string(),
                task_id: "receipts-task".into(),
                limit: 1000,
                ..Default::default()
            }),
        )
        .await;
        assert_eq!(exported.event_type, "receipts.exported");
        let exported_payload: serde_json::Value =
            serde_json::from_slice(&exported.payload).unwrap();
        assert_eq!(exported_payload["ok"], true, "{exported_payload:?}");
        assert_eq!(exported_payload["actual_exported_count"], 2);
        assert!(destination.join("manifest.json").exists());
        assert!(destination.join("receipts.jsonl").exists());

        let _ = std::fs::remove_dir_all(&data_root);
    }

    // ------------------------------------------------------------------
    // Постоянное слушание (план 04.5): девять команд и их коды ошибок.
    // ------------------------------------------------------------------

    /// Мост поверх временной базы и временного каталога данных.
    ///
    /// Каталог берётся полем, а не переменной окружения: подмена окружения
    /// сделала бы соседние тесты зависимыми от порядка запуска.
    fn ambient_bridge(name: &str) -> (IpcBridge, tempfile::TempDir) {
        let directory = tempfile::tempdir().expect("temp dir");
        let journal =
            EventJournal::open(directory.path().join(format!("{name}.db"))).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator)
            .with_ambient_data_dir(directory.path().to_path_buf());
        (bridge, directory)
    }

    async fn ambient_call(
        bridge: &IpcBridge,
        command: generated::command_envelope::Command,
    ) -> (String, serde_json::Value) {
        let (mut client, server) = duplex(256 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "ambient-request".into(),
            client_id: "ambient-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(command),
        };
        transport::write_frame(&mut client, &envelope.encode_to_vec())
            .await
            .expect("request writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("request serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        let payload = serde_json::from_slice(&event.payload).unwrap_or(serde_json::Value::Null);
        (event.event_type, payload)
    }

    async fn typed_checkpoint_call(
        bridge: &IpcBridge,
        command: generated::command_envelope::Command,
    ) -> generated::EventEnvelope {
        let (mut client, server) = duplex(256 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: uuid::Uuid::now_v7().to_string(),
            client_id: "checkpoint-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(command),
        };
        transport::write_frame(&mut client, &envelope.encode_to_vec())
            .await
            .expect("typed checkpoint request writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("typed checkpoint request serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("typed checkpoint response reads");
        generated::EventEnvelope::decode(response.as_slice()).expect("typed checkpoint decodes")
    }

    async fn typed_goal_call(
        bridge: &IpcBridge,
        request_id: &str,
        command: generated::command_envelope::Command,
    ) -> generated::EventEnvelope {
        let (mut client, server) = duplex(256 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: request_id.into(),
            client_id: "goal-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(command),
        };
        transport::write_frame(&mut client, &envelope.encode_to_vec())
            .await
            .expect("typed goal request writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("typed goal request serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("typed goal response reads");
        generated::EventEnvelope::decode(response.as_slice()).expect("typed goal decodes")
    }

    #[tokio::test]
    async fn persistent_goal_ipc_is_typed_bounded_and_recoverable() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-goal-{}.db", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let bridge = IpcBridge::new(journal);
        let workspace = std::env::temp_dir().join("evohime-goal-workspace");
        std::fs::create_dir_all(&workspace).expect("goal workspace creates");
        let create = generated::CreateGoal {
            goal_id: "goal-ipc-1".into(),
            workspace_path: workspace.to_string_lossy().into_owned(),
            chat_id: "chat-1".into(),
            objective: "Проверить typed Goal".into(),
            success_criteria: vec![generated::GoalCriterionInput {
                id: "criterion-1".into(),
                kind: "manual".into(),
                statement: "Core evidence сохранено".into(),
            }],
            idempotency_key: "goal-create-1".into(),
            ..Default::default()
        };
        let created = typed_goal_call(
            &bridge,
            "goal-create-request",
            generated::command_envelope::Command::CreateGoal(create.clone()),
        )
        .await;
        let created = match created.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed GoalAction, got {other:?}"),
        };
        assert!(
            created.applied,
            "create error={} message={}",
            created.error_code, created.error_message
        );
        assert_eq!(created.goal_version, 1);
        let projection = created.goal.expect("create carries projection");
        assert_eq!(projection.status, "active");
        assert_eq!(projection.remaining_criteria, vec!["criterion-1"]);
        assert!(!projection.workspace_id.contains("evohime-goal-workspace"));

        let replay = typed_goal_call(
            &bridge,
            "goal-create-request",
            generated::command_envelope::Command::CreateGoal(create),
        )
        .await;
        let replay = match replay.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed replay GoalAction, got {other:?}"),
        };
        assert!(replay.deduplicated);
        assert_eq!(replay.goal_version, 1);

        let listed = typed_goal_call(
            &bridge,
            "goal-list-request",
            generated::command_envelope::Command::ListGoals(generated::ListGoals {
                workspace_path: workspace.to_string_lossy().into_owned(),
                limit: 16,
            }),
        )
        .await;
        let listed = match listed.event {
            Some(generated::event_envelope::Event::GoalList(result)) => result,
            other => panic!("expected typed GoalList, got {other:?}"),
        };
        assert_eq!(listed.goals.len(), 1);
        assert_eq!(listed.goals[0].objective, "Проверить typed Goal");

        let fetched = typed_goal_call(
            &bridge,
            "goal-get-request",
            generated::command_envelope::Command::GetGoal(generated::GetGoal {
                goal_id: "goal-ipc-1".into(),
            }),
        )
        .await;
        let fetched = match fetched.event {
            Some(generated::event_envelope::Event::Goal(goal)) => goal,
            other => panic!("expected typed Goal projection, got {other:?}"),
        };
        assert_eq!(fetched.objective, "Проверить typed Goal");

        let updated = typed_goal_call(
            &bridge,
            "goal-update-request",
            generated::command_envelope::Command::UpdateGoal(generated::UpdateGoal {
                goal_id: "goal-ipc-1".into(),
                expected_version: 1,
                objective: "Проверить typed Goal и историю".into(),
                idempotency_key: "goal-update-key".into(),
                ..Default::default()
            }),
        )
        .await;
        let updated = match updated.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed update result, got {other:?}"),
        };
        assert!(updated.applied);
        assert_eq!(updated.goal_version, 2);

        let paused = typed_goal_call(
            &bridge,
            "goal-pause-request",
            generated::command_envelope::Command::PauseGoal(generated::GoalAction {
                goal_id: "goal-ipc-1".into(),
                expected_version: 2,
                idempotency_key: "goal-pause-key".into(),
            }),
        )
        .await;
        let paused = match paused.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed pause result, got {other:?}"),
        };
        assert_eq!(
            paused.goal.as_ref().map(|goal| goal.status.as_str()),
            Some("paused")
        );

        let resumed = typed_goal_call(
            &bridge,
            "goal-resume-request",
            generated::command_envelope::Command::ResumeGoal(generated::GoalAction {
                goal_id: "goal-ipc-1".into(),
                expected_version: 3,
                idempotency_key: "goal-resume-key".into(),
            }),
        )
        .await;
        let resumed = match resumed.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed resume result, got {other:?}"),
        };
        assert_eq!(
            resumed.goal.as_ref().map(|goal| goal.status.as_str()),
            Some("active")
        );

        let checkpoint = crate::task_checkpoint::TaskCheckpointRuntime::new(bridge.journal.clone())
            .capture(
                "goal-checkpoint-task",
                &workspace,
                crate::task_checkpoint::CheckpointStatus::Blocked,
                crate::task_checkpoint::CheckpointCaptureReason::RecoveryBlocked,
                None,
            )
            .await
            .expect("goal checkpoint persists");
        let linked = typed_goal_call(
            &bridge,
            "goal-link-checkpoint-request",
            generated::command_envelope::Command::LinkGoalReference(generated::LinkGoalReference {
                goal_id: "goal-ipc-1".into(),
                expected_version: 4,
                kind: "checkpoint".into(),
                reference_id: checkpoint.id,
                idempotency_key: "goal-link-checkpoint-key".into(),
            }),
        )
        .await;
        let linked = match linked.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed checkpoint link result, got {other:?}"),
        };
        assert!(linked.applied);
        assert_eq!(linked.goal_version, 5);

        let missing_link = typed_goal_call(
            &bridge,
            "goal-link-missing-request",
            generated::command_envelope::Command::LinkGoalReference(generated::LinkGoalReference {
                goal_id: "goal-ipc-1".into(),
                expected_version: 5,
                kind: "workflow".into(),
                reference_id: "missing-workflow".into(),
                idempotency_key: "goal-link-missing-key".into(),
            }),
        )
        .await;
        let missing_link = match missing_link.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed link result, got {other:?}"),
        };
        assert_eq!(missing_link.error_code, "reference_not_found");

        let stale = typed_goal_call(
            &bridge,
            "goal-pause-stale",
            generated::command_envelope::Command::PauseGoal(generated::GoalAction {
                goal_id: "goal-ipc-1".into(),
                expected_version: 99,
                idempotency_key: "goal-pause-stale-key".into(),
            }),
        )
        .await;
        let stale = match stale.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed stale GoalAction, got {other:?}"),
        };
        assert_eq!(stale.error_code, "stale_version");

        let verified = typed_goal_call(
            &bridge,
            "goal-verify-request",
            generated::command_envelope::Command::VerifyGoalCriterion(
                generated::VerifyGoalCriterion {
                    goal_id: "goal-ipc-1".into(),
                    expected_version: 5,
                    criterion_id: "criterion-1".into(),
                    idempotency_key: "goal-verify-key".into(),
                },
            ),
        )
        .await;
        let verified = match verified.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed verified GoalAction, got {other:?}"),
        };
        assert!(verified.applied);
        assert_eq!(
            verified.goal.as_ref().map(|goal| goal.status.as_str()),
            Some("completed")
        );
        let verified_criterion = &verified
            .goal
            .as_ref()
            .expect("verified projection")
            .success_criteria[0];
        assert_eq!(verified_criterion.provenance, "core");
        assert!(verified_criterion
            .evidence_ref
            .starts_with("core:user-decision:"));

        let cancelled = typed_goal_call(
            &bridge,
            "goal-cancel-completed",
            generated::command_envelope::Command::CancelGoal(generated::GoalAction {
                goal_id: "goal-ipc-1".into(),
                expected_version: 6,
                idempotency_key: "goal-cancel-completed-key".into(),
            }),
        )
        .await;
        let cancelled = match cancelled.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed cancel result, got {other:?}"),
        };
        assert_eq!(cancelled.error_code, "invalid_state_transition");

        let cancel_target = typed_goal_call(
            &bridge,
            "goal-create-cancel-target",
            generated::command_envelope::Command::CreateGoal(generated::CreateGoal {
                goal_id: "goal-ipc-cancel-target".into(),
                workspace_path: workspace.to_string_lossy().into_owned(),
                objective: "Отменяемая цель".into(),
                success_criteria: vec![generated::GoalCriterionInput {
                    id: "criterion-1".into(),
                    kind: "manual".into(),
                    statement: "Не требуется подтверждение".into(),
                }],
                idempotency_key: "goal-create-cancel-target-key".into(),
                ..Default::default()
            }),
        )
        .await;
        let cancel_target = match cancel_target.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected cancel target creation, got {other:?}"),
        };
        assert!(cancel_target.applied);
        let cancelled = typed_goal_call(
            &bridge,
            "goal-cancel-active",
            generated::command_envelope::Command::CancelGoal(generated::GoalAction {
                goal_id: "goal-ipc-cancel-target".into(),
                expected_version: 1,
                idempotency_key: "goal-cancel-active-key".into(),
            }),
        )
        .await;
        let cancelled = match cancelled.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected successful cancel result, got {other:?}"),
        };
        assert_eq!(
            cancelled.goal.as_ref().map(|goal| goal.status.as_str()),
            Some("cancelled")
        );
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[tokio::test]
    async fn task_checkpoint_ipc_is_typed_bounded_and_idempotent() {
        let directory = tempfile::tempdir().expect("temp dir");
        let journal =
            EventJournal::open(directory.path().join("checkpoint-ipc.db")).expect("journal opens");
        let runtime = crate::task_checkpoint::TaskCheckpointRuntime::new(journal.clone());
        let checkpoint = runtime
            .capture(
                "task-1",
                directory.path(),
                crate::task_checkpoint::CheckpointStatus::Blocked,
                crate::task_checkpoint::CheckpointCaptureReason::RecoveryBlocked,
                None,
            )
            .await
            .expect("checkpoint persists");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);

        let projection_event = typed_checkpoint_call(
            &bridge,
            generated::command_envelope::Command::GetTaskCheckpoint(generated::GetTaskCheckpoint {
                task_id: "task-1".into(),
                workspace_path: directory.path().to_string_lossy().into_owned(),
                max_replay_events: 64,
            }),
        )
        .await;
        assert!(projection_event.payload.is_empty());
        let Some(generated::event_envelope::Event::TaskCheckpoint(projection)) =
            projection_event.event
        else {
            panic!("expected typed checkpoint projection");
        };
        assert_eq!(projection.checkpoint_id, checkpoint.id);
        assert_eq!(projection.recovery_disposition, "blocked");
        assert!(projection
            .refs
            .iter()
            .all(|reference| reference.content_hash.len() <= 128));

        let action = generated::ResolveTaskCheckpoint {
            task_id: "task-1".into(),
            workspace_path: directory.path().to_string_lossy().into_owned(),
            checkpoint_id: checkpoint.id.clone(),
            expected_source_event_seq: checkpoint.source_event_seq,
            action: "acknowledge_recovery".into(),
            idempotency_key: "ack-1".into(),
        };
        let first_action = typed_checkpoint_call(
            &bridge,
            generated::command_envelope::Command::ResolveTaskCheckpoint(action.clone()),
        )
        .await;
        let Some(generated::event_envelope::Event::TaskCheckpointActionResult(first_result)) =
            first_action.event
        else {
            panic!("expected typed checkpoint action result");
        };
        assert!(first_result.applied);
        assert!(!first_result.deduplicated);
        assert!(first_action.payload.is_empty());

        let repeated_action = typed_checkpoint_call(
            &bridge,
            generated::command_envelope::Command::ResolveTaskCheckpoint(action),
        )
        .await;
        let Some(generated::event_envelope::Event::TaskCheckpointActionResult(repeated_result)) =
            repeated_action.event
        else {
            panic!("expected deduplicated checkpoint action result");
        };
        assert!(repeated_result.applied);
        assert!(repeated_result.deduplicated);
        let action_events = journal
            .task_history("task-1", 32)
            .await
            .expect("checkpoint history reads")
            .into_iter()
            .filter(|event| event.event_type == "task.checkpoint.action")
            .count();
        assert_eq!(action_events, 1);
    }

    #[tokio::test]
    async fn agent_skills_ipc_is_typed_metadata_first_and_non_durable() {
        let directory = tempfile::tempdir().expect("temp dir");
        let skill_dir = directory.path().join(".agents/skills/reviewer");
        std::fs::create_dir_all(skill_dir.join("references")).expect("skill dir creates");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: reviewer\ndescription: bounded review\nversion: 1.0.0\n---\nsecretly never persisted\n",
        )
        .expect("skill writes");
        std::fs::write(skill_dir.join("references/guide.md"), "bounded guide")
            .expect("reference writes");
        let journal =
            EventJournal::open(directory.path().join("skills-ipc.db")).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let workspace = directory.path().to_string_lossy().into_owned();

        let catalog_event = typed_checkpoint_call(
            &bridge,
            generated::command_envelope::Command::ListSkills(generated::ListSkills {
                workspace_path: workspace.clone(),
                limit: 10,
            }),
        )
        .await;
        assert!(catalog_event.payload.is_empty());
        let Some(generated::event_envelope::Event::SkillCatalog(catalog)) = catalog_event.event
        else {
            panic!("expected typed skill catalog");
        };
        assert_eq!(catalog.skills.len(), 1);
        assert_eq!(catalog.skills[0].skill_id, "reviewer");
        assert!(catalog.skills[0].content_hash.len() <= 128);

        let content_event = typed_checkpoint_call(
            &bridge,
            generated::command_envelope::Command::LoadSkill(generated::LoadSkill {
                workspace_path: workspace,
                skill_id: "reviewer".into(),
                max_bytes: 4096,
            }),
        )
        .await;
        let Some(generated::event_envelope::Event::SkillContent(content)) = content_event.event
        else {
            panic!("expected typed skill content");
        };
        assert_eq!(content.error_code, "");
        assert!(content.content.contains("secretly never persisted"));
        assert!(content_event.payload.is_empty());
        let history = journal
            .task_history("skill:reviewer", 16)
            .await
            .expect("skill trace reads");
        assert_eq!(history.len(), 1);
        assert!(!String::from_utf8_lossy(&history[0].payload).contains("secretly never persisted"));
    }

    #[tokio::test]
    async fn a_voice_command_card_appears_and_is_declined_without_launching_anything() {
        let (bridge, _directory) = ambient_bridge("ambient-voice");
        let policy = evohime_listener_contract::AmbientPolicy::default();
        let now_ms = crate::task_memory::now_millis();
        let decision = crate::voice_command::decide(
            &bridge.voice_commands(),
            &policy,
            "Ева, открой блокнот",
            now_ms,
            "voice-1".to_owned(),
        );
        let crate::voice_command::Decision::Confirm(command) = decision else {
            panic!("услышанное обязано ждать клика");
        };
        assert_eq!(command.app_id, "notepad");

        let (event_type, listed) = ambient_call(
            &bridge,
            generated::command_envelope::Command::ListVoiceCommands(generated::ListVoiceCommands {
                limit: 10,
            }),
        )
        .await;
        assert_eq!(event_type, "ambient.voice_commands");
        assert_eq!(listed["requires_confirmation"], true);
        assert_eq!(listed["commands"][0]["command_id"], "voice-1");
        assert_eq!(listed["commands"][0]["title"], "Блокнот");

        let (event_type, declined) = ambient_call(
            &bridge,
            generated::command_envelope::Command::ResolveVoiceCommand(
                generated::ResolveVoiceCommand {
                    command_id: "voice-1".into(),
                    accepted: false,
                },
            ),
        )
        .await;
        assert_eq!(event_type, "ambient.voice_command_resolved");
        assert_eq!(declined["launched"], false);
        assert_eq!(declined["state"], "declined");

        // Второй клик по решённой карточке ничего не запускает: её больше нет.
        let (_, again) = ambient_call(
            &bridge,
            generated::command_envelope::Command::ResolveVoiceCommand(
                generated::ResolveVoiceCommand {
                    command_id: "voice-1".into(),
                    accepted: true,
                },
            ),
        )
        .await;
        assert_eq!(again["launched"], false);
        assert_eq!(again["error_code"], "not_found");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn saving_a_policy_without_the_voice_fields_keeps_the_stored_value() {
        let (bridge, _directory) = ambient_bridge("ambient-voice-policy");
        let _control = attach_fake_listener(&bridge);
        let (_, saved) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SaveAmbientPolicy(generated::SaveAmbientPolicy {
                policy: Some(generated::AmbientPolicy {
                    quiet_hours: Vec::new(),
                    blocklist_patterns: Vec::new(),
                    retention_days: 7,
                    window_title_blocklist: Vec::new(),
                    voice_commands: Some(false),
                    voice_commands_autorun: None,
                }),
            }),
        )
        .await;
        assert_eq!(saved["applied"], true);
        let (_, policy) = ambient_call(
            &bridge,
            generated::command_envelope::Command::GetAmbientPolicy(generated::GetAmbientPolicy {}),
        )
        .await;
        assert_eq!(policy["voice_commands"], false);
        assert_eq!(policy["voice_commands_autorun"], false);

        // Старый клиент не шлёт новых полей — и не выключает их своим молчанием.
        let (_, saved) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SaveAmbientPolicy(generated::SaveAmbientPolicy {
                policy: Some(generated::AmbientPolicy {
                    quiet_hours: Vec::new(),
                    blocklist_patterns: vec!["zoom*.exe".into()],
                    retention_days: 7,
                    window_title_blocklist: Vec::new(),
                    voice_commands: None,
                    voice_commands_autorun: None,
                }),
            }),
        )
        .await;
        assert_eq!(saved["applied"], true);
        let (_, policy) = ambient_call(
            &bridge,
            generated::command_envelope::Command::GetAmbientPolicy(generated::GetAmbientPolicy {}),
        )
        .await;
        assert_eq!(policy["voice_commands"], false);
    }

    /// Подключает фиктивный листенер: команда уезжает в канал и остаётся там.
    fn attach_fake_listener(
        bridge: &IpcBridge,
    ) -> tokio::sync::mpsc::Receiver<crate::ambient::ListenerControl> {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let registry = bridge.ambient();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(registry.attach_control(tx))
        });
        rx
    }

    /// Без листенера включение не притворяется успехом: намерение сохранено,
    /// но состояние честно называется недоступным.
    #[tokio::test]
    async fn enabling_without_a_listener_reports_that_the_listener_is_missing() {
        let (bridge, directory) = ambient_bridge("ambient-no-listener");
        let (event_type, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SetAmbientListening(
                generated::SetAmbientListening {
                    enabled: true,
                    paused: false,
                    device_id: String::new(),
                },
            ),
        )
        .await;
        assert_eq!(event_type, "ambient.listening");
        assert_eq!(payload["error_code"], "LISTENER_UNAVAILABLE");
        assert_eq!(payload["state"], "engine_unavailable");
        // Намерение всё равно сохранено: следующее подключение листенера его
        // применит, а не начнёт с выключенного микрофона.
        assert!(crate::ambient::load_control(directory.path()).enabled);
    }

    /// Движок не готов — включение отвечает `ENGINE_NOT_READY`, а не молчит.
    #[tokio::test(flavor = "multi_thread")]
    async fn enabling_without_an_engine_reports_engine_not_ready() {
        let (bridge, _directory) = ambient_bridge("ambient-engine");
        let mut control = attach_fake_listener(&bridge);
        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SetAmbientListening(
                generated::SetAmbientListening {
                    enabled: true,
                    paused: false,
                    device_id: String::new(),
                },
            ),
        )
        .await;
        assert_eq!(payload["error_code"], "ENGINE_NOT_READY");
        assert_eq!(payload["state"], "starting");
        assert!(matches!(
            control.try_recv(),
            Ok(crate::ambient::ListenerControl::Policy(_))
        ));
    }

    /// Занятое устройство называется своим кодом и не превращается в
    /// «запускаюсь».
    #[tokio::test(flavor = "multi_thread")]
    async fn a_busy_device_reports_a_conflict() {
        let (bridge, _directory) = ambient_bridge("ambient-conflict");
        let _control = attach_fake_listener(&bridge);
        bridge
            .ambient()
            .set_state(
                ListeningState::DeviceConflict,
                ListeningReason::DeviceConflict,
                None,
            )
            .await;
        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SetAmbientListening(
                generated::SetAmbientListening {
                    enabled: true,
                    paused: false,
                    device_id: String::new(),
                },
            ),
        )
        .await;
        assert_eq!(payload["error_code"], "DEVICE_CONFLICT");
        assert_eq!(payload["state"], "device_conflict");
    }

    /// Неизвестное устройство не выбирается: подмена на умолчание означала бы
    /// слушать не тем микрофоном, который выбрал пользователь.
    #[tokio::test(flavor = "multi_thread")]
    async fn selecting_a_missing_device_is_refused() {
        let (bridge, _directory) = ambient_bridge("ambient-device");
        let _control = attach_fake_listener(&bridge);
        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SetAmbientListening(
                generated::SetAmbientListening {
                    enabled: true,
                    paused: false,
                    device_id: "mic-that-left".into(),
                },
            ),
        )
        .await;
        assert_eq!(payload["error_code"], "DEVICE_DISCONNECTED");
    }

    /// Фраза в поле идентификатора устройства — это попытка протащить текст
    /// через метаданные, и она отбивается контрактом 04.1.
    #[tokio::test]
    async fn a_phrase_in_a_device_id_is_refused() {
        let (bridge, _directory) = ambient_bridge("ambient-device-id");
        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SetAmbientListening(
                generated::SetAmbientListening {
                    enabled: true,
                    paused: false,
                    device_id: "позвони маме завтра".into(),
                },
            ),
        )
        .await;
        assert_eq!(payload["error_code"], "INVALID_ARGUMENT");
    }

    /// Снимок статуса отвечает всегда: панель открывается, не дожидаясь
    /// события.
    #[tokio::test]
    async fn status_answers_before_any_event_arrives() {
        let (bridge, _directory) = ambient_bridge("ambient-status");
        let (event_type, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::GetAmbientStatus(generated::GetAmbientStatus {}),
        )
        .await;
        assert_eq!(event_type, "ambient.status");
        assert_eq!(payload["state"], "engine_unavailable");
        assert_eq!(payload["engine_ready"], false);
        assert!(payload["devices"].as_array().expect("devices").is_empty());
    }

    /// Список эпизодов не несёт текста; текст отдаётся только явным запросом
    /// одного эпизода.
    #[tokio::test]
    async fn text_is_absent_from_the_listing_and_present_only_on_demand() {
        let (bridge, _directory) = ambient_bridge("ambient-episodes");
        let journal = bridge.journal();
        journal
            .open_ambient_episode(
                "ep-1",
                "whisper-base-q5_1",
                "whisper-base-q5_1",
                evohime_listener_contract::ExtractionState::Disabled,
                1_700_000_000_000,
            )
            .await
            .expect("episode opens");
        journal
            .insert_ambient_utterance(
                &crate::ambient::AmbientUtteranceInput {
                    utterance_id: "ep-1-0".into(),
                    episode_id: "ep-1".into(),
                    sequence: 0,
                    started_at_ms: 1_700_000_000_000,
                    duration_ms: 1_200,
                    text: "надо купить хлеб".into(),
                    language: "ru".into(),
                    avg_logprob: -0.2,
                    redacted: false,
                },
                7,
                2_000,
            )
            .await
            .expect("utterance stored");

        let (event_type, listing) = ambient_call(
            &bridge,
            generated::command_envelope::Command::ListAmbientEpisodes(
                generated::ListAmbientEpisodes {
                    since_ms: 0,
                    limit: 10,
                    cursor: String::new(),
                },
            ),
        )
        .await;
        assert_eq!(event_type, "ambient.episodes");
        let serialized = listing.to_string();
        assert!(
            !serialized.contains("надо купить хлеб"),
            "listing leaked transcript text"
        );
        assert_eq!(listing["episodes"][0]["episode_id"], "ep-1");
        assert_eq!(listing["episodes"][0]["utterance_count"], 1);

        let (event_type, detail) = ambient_call(
            &bridge,
            generated::command_envelope::Command::GetAmbientEpisode(generated::GetAmbientEpisode {
                episode_id: "ep-1".into(),
            }),
        )
        .await;
        assert_eq!(event_type, "ambient.episode");
        assert_eq!(detail["utterances"][0]["text"], "надо купить хлеб");
    }

    /// Неподтверждённое удаление отвергается ядром, а не только модальным
    /// окном оболочки: обход UI не даёт больше прав.
    #[tokio::test]
    async fn deleting_without_confirmation_is_refused_by_core() {
        let (bridge, _directory) = ambient_bridge("ambient-delete");
        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::DeleteAmbientTranscripts(
                generated::DeleteAmbientTranscripts {
                    episode_ids: vec!["ep-1".into()],
                    all: false,
                    confirmed: false,
                },
            ),
        )
        .await;
        assert_eq!(payload["error_code"], "CONFIRMATION_REQUIRED");
        assert_eq!(payload["deleted_count"], 0);

        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::ForgetAmbientWindow(
                generated::ForgetAmbientWindow {
                    window_ms: 5 * 60 * 1000,
                    confirmed: false,
                },
            ),
        )
        .await;
        assert_eq!(payload["error_code"], "CONFIRMATION_REQUIRED");
    }

    /// Удаление действительно удаляет текст и вычищает ambient-строки
    /// журнала: событие об эпизоде не переживает сам эпизод.
    #[tokio::test]
    async fn deleting_removes_the_text_and_its_journal_rows() {
        let (bridge, _directory) = ambient_bridge("ambient-delete-real");
        let journal = bridge.journal();
        journal
            .open_ambient_episode(
                "ep-2",
                "whisper-base-q5_1",
                "whisper-base-q5_1",
                evohime_listener_contract::ExtractionState::Disabled,
                1_700_000_000_000,
            )
            .await
            .expect("episode opens");
        journal
            .insert_ambient_utterance(
                &crate::ambient::AmbientUtteranceInput {
                    utterance_id: "ep-2-0".into(),
                    episode_id: "ep-2".into(),
                    sequence: 0,
                    started_at_ms: 1_700_000_000_000,
                    duration_ms: 900,
                    text: "это надо забыть".into(),
                    language: "ru".into(),
                    avg_logprob: -0.1,
                    redacted: false,
                },
                7,
                2_000,
            )
            .await
            .expect("utterance stored");
        bridge
            .publish_ambient(&evohime_listener_contract::AmbientLogEvent::Transcript {
                episode_id: evohime_listener_contract::EpisodeId::new("ep-2").unwrap(),
                started_at_ms: 1_700_000_000_000,
                utterance_count: 1,
                extraction_state: evohime_listener_contract::ExtractionState::Disabled,
            })
            .await
            .expect("transcript event published");

        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::DeleteAmbientTranscripts(
                generated::DeleteAmbientTranscripts {
                    episode_ids: vec!["ep-2".into()],
                    all: false,
                    confirmed: true,
                },
            ),
        )
        .await;
        assert_eq!(payload["deleted_count"], 1);
        assert!(journal
            .list_ambient_utterances("ep-2", 10)
            .await
            .expect("utterances read")
            .is_empty());
        let replay = journal
            .replay_bounded(0, 256)
            .await
            .expect("journal replays");
        assert!(
            !replay
                .events
                .iter()
                .any(|event| event.task_id == "ep-2" && event.event_type == "ambient.transcript"),
            "episode journal rows outlived the episode"
        );
    }

    /// Ни одно ambient-событие не несёт ни текста, ни его хеша.
    #[tokio::test(flavor = "multi_thread")]
    async fn ambient_events_never_carry_text_or_its_hash() {
        let (bridge, _directory) = ambient_bridge("ambient-events");
        let _control = attach_fake_listener(&bridge);
        let _ = ambient_call(
            &bridge,
            generated::command_envelope::Command::SetAmbientListening(
                generated::SetAmbientListening {
                    enabled: true,
                    paused: true,
                    device_id: String::new(),
                },
            ),
        )
        .await;
        let replay = bridge
            .journal()
            .replay_bounded(0, 256)
            .await
            .expect("journal replays");
        let ambient_rows: Vec<_> = replay
            .events
            .iter()
            .filter(|event| event.event_type.starts_with("ambient."))
            .collect();
        assert!(!ambient_rows.is_empty(), "no ambient event was published");
        for event in ambient_rows {
            let payload: serde_json::Value =
                serde_json::from_slice(&event.payload).expect("ambient payload is json");
            let object = payload.as_object().expect("ambient payload is an object");
            for forbidden in ["text", "text_hash", "transcript", "utterance"] {
                assert!(
                    !object.contains_key(forbidden),
                    "{} leaked {forbidden}",
                    event.event_type
                );
            }
        }
    }

    /// Политика применяется целиком или не применяется вовсе.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_invalid_policy_is_refused_whole() {
        let (bridge, directory) = ambient_bridge("ambient-policy");
        let mut control = attach_fake_listener(&bridge);

        let (event_type, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SaveAmbientPolicy(generated::SaveAmbientPolicy {
                policy: Some(generated::AmbientPolicy {
                    quiet_hours: vec![generated::QuietHours {
                        start_minute: 23 * 60,
                        end_minute: 7 * 60,
                    }],
                    blocklist_patterns: vec!["zoom*.exe".into()],
                    retention_days: 14,
                    window_title_blocklist: vec!["*банк*".into()],
                    voice_commands: None,
                    voice_commands_autorun: None,
                }),
            }),
        )
        .await;
        assert_eq!(event_type, "ambient.policy_saved");
        assert_eq!(payload["applied"], true);
        assert!(matches!(
            control.try_recv(),
            Ok(crate::ambient::ListenerControl::Policy(_))
        ));

        let (_, refused) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SaveAmbientPolicy(generated::SaveAmbientPolicy {
                policy: Some(generated::AmbientPolicy {
                    quiet_hours: Vec::new(),
                    blocklist_patterns: vec!["^bank.*$".into()],
                    retention_days: 14,
                    window_title_blocklist: Vec::new(),
                    voice_commands: None,
                    voice_commands_autorun: None,
                }),
            }),
        )
        .await;
        assert_eq!(refused["applied"], false);
        assert_eq!(refused["error_code"], "INVALID_ARGUMENT");

        let (_, over_retention) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SaveAmbientPolicy(generated::SaveAmbientPolicy {
                policy: Some(generated::AmbientPolicy {
                    quiet_hours: Vec::new(),
                    blocklist_patterns: Vec::new(),
                    retention_days: 365,
                    window_title_blocklist: Vec::new(),
                    voice_commands: None,
                    voice_commands_autorun: None,
                }),
            }),
        )
        .await;
        assert_eq!(over_retention["error_code"], "POLICY_INVALID");

        // Отвергнутая политика не затёрла сохранённую.
        let stored = crate::ambient::load_policy(directory.path());
        assert_eq!(stored.retention_days, 14);
        assert_eq!(stored.process_blocklist, vec!["zoom*.exe".to_string()]);

        let (event_type, read_back) = ambient_call(
            &bridge,
            generated::command_envelope::Command::GetAmbientPolicy(generated::GetAmbientPolicy {}),
        )
        .await;
        assert_eq!(event_type, "ambient.policy");
        assert_eq!(read_back["retention_days"], 14);
        assert_eq!(read_back["quiet_hours"][0]["start_minute"], 23 * 60);
    }

    /// Кладёт готовое предложение в базу моста.
    async fn seed_proposal(
        bridge: &IpcBridge,
        proposal_id: &str,
        kind: evohime_listener_contract::ProposalKind,
        subject: &str,
        episode_id: Option<&str>,
        now_ms: u64,
    ) {
        use crate::ambient_proactivity as proactivity;
        let subject_key = proactivity::subject_key(subject);
        let proposal_key = proactivity::proposal_key(kind, &subject_key, now_ms);
        let mute_key = proactivity::mute_key(kind, &subject_key);
        let record = crate::ambient::proposal_record(crate::ambient::ProposalRecordInput {
            proposal_id,
            proposal_key: &proposal_key,
            mute_key: &mute_key,
            kind,
            subject_key: &subject_key,
            subject,
            title: "Напомнить купить хлеб",
            source_episode_id: episode_id,
            now_ms,
        });
        bridge
            .journal()
            .record_ambient_proposal(&record)
            .await
            .expect("предложение записывается");
    }

    fn resolve_command(
        proposal_id: &str,
        accepted: bool,
        mute: bool,
        idempotency_key: &str,
    ) -> generated::command_envelope::Command {
        generated::command_envelope::Command::ResolveAmbientProposal(
            generated::ResolveAmbientProposal {
                proposal_id: proposal_id.into(),
                accepted,
                idempotency_key: idempotency_key.into(),
                mute,
            },
        )
    }

    /// Решения по несуществующему предложению не бывает: команда честно
    /// отвечает «не применено», а не выдумывает успех. Пустой ключ
    /// идемпотентности отвергается там же.
    #[tokio::test]
    async fn resolving_an_unknown_proposal_is_not_applied() {
        let (bridge, _directory) = ambient_bridge("ambient-proposal-unknown");
        let (event_type, payload) =
            ambient_call(&bridge, resolve_command("prop-1", true, false, "idem-1")).await;
        assert_eq!(event_type, "ambient.proposal_resolved");
        assert_eq!(payload["applied"], false);
        assert_eq!(payload["error_code"], "INVALID_ARGUMENT");

        seed_proposal(
            &bridge,
            "prop-1",
            evohime_listener_contract::ProposalKind::Reminder,
            "хлеб",
            None,
            crate::task_memory::now_millis(),
        )
        .await;
        let (_, without_key) =
            ambient_call(&bridge, resolve_command("prop-1", true, false, "   ")).await;
        assert_eq!(
            without_key["applied"], false,
            "принятие без ключа идемпотентности не проходит"
        );
        assert_eq!(without_key["error_code"], "INVALID_ARGUMENT");
    }

    /// Повторный клик по карточке возвращает первое решение и не создаёт
    /// вторую задачу.
    #[tokio::test]
    async fn a_repeated_resolve_with_the_same_key_creates_no_second_task() {
        let (bridge, _directory) = ambient_bridge("ambient-proposal-idempotent");
        seed_proposal(
            &bridge,
            "prop-1",
            evohime_listener_contract::ProposalKind::Suggestion,
            "отчёт",
            None,
            crate::task_memory::now_millis(),
        )
        .await;
        let (_, first) =
            ambient_call(&bridge, resolve_command("prop-1", true, false, "idem-1")).await;
        assert_eq!(first["applied"], true);
        assert_eq!(first["state"], "accepted");
        let task_id = first["task_id"]
            .as_str()
            .expect("задача создана")
            .to_owned();
        assert!(!task_id.is_empty());

        let (_, second) =
            ambient_call(&bridge, resolve_command("prop-1", true, false, "idem-1")).await;
        assert_eq!(second["applied"], true, "повтор отвечает первым решением");
        assert_eq!(second["task_id"], task_id);

        let tasks = bridge
            .journal()
            .list_work_items(AMBIENT_PROPOSAL_PROJECT_ID)
            .await
            .expect("задачи читаются");
        assert_eq!(tasks.len(), 1, "двойной клик не породил вторую задачу");
        assert_eq!(tasks[0].status, "backlog", "принятое не запускается само");
    }

    /// Принятое напоминание — неисполняемая запись: это записано в данных, а
    /// не подразумевается. Провенанс ведёт к эпизоду-источнику.
    #[tokio::test]
    async fn an_accepted_reminder_is_a_non_executable_row_with_provenance() {
        let (bridge, _directory) = ambient_bridge("ambient-proposal-reminder");
        let now_ms = crate::task_memory::now_millis();
        bridge
            .journal()
            .open_ambient_episode(
                "ep-1",
                "whisper-base-q5_1",
                "base-q5_1",
                evohime_listener_contract::ExtractionState::Done,
                now_ms,
            )
            .await
            .expect("эпизод открывается");
        seed_proposal(
            &bridge,
            "prop-1",
            evohime_listener_contract::ProposalKind::Reminder,
            "хлеб",
            Some("ep-1"),
            now_ms,
        )
        .await;
        let (_, payload) =
            ambient_call(&bridge, resolve_command("prop-1", true, false, "idem-1")).await;
        assert_eq!(payload["applied"], true);
        let tasks = bridge
            .journal()
            .list_work_items(AMBIENT_PROPOSAL_PROJECT_ID)
            .await
            .expect("задачи читаются");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].non_goals, AMBIENT_REMINDER_NON_GOAL);
        assert_eq!(tasks[0].source_ref.as_deref(), Some("ep-1"));
    }

    /// Отклонение задачу не создаёт, а mute переживает рестарт Core: он живёт
    /// строкой таблицы, а не полем реестра в памяти процесса.
    #[tokio::test]
    async fn muting_a_subject_survives_a_core_restart() {
        let directory = tempfile::tempdir().expect("temp dir");
        let database = directory.path().join("ambient-proposal-mute.db");
        let now_ms = crate::task_memory::now_millis();
        {
            let journal = EventJournal::open(&database).expect("journal opens");
            let (coordinator, _events) =
                TaskCoordinator::new_with_journal(8, None, journal.clone());
            let bridge = IpcBridge::with_coordinator(journal, coordinator)
                .with_ambient_data_dir(directory.path().to_path_buf());
            seed_proposal(
                &bridge,
                "prop-1",
                evohime_listener_contract::ProposalKind::Reminder,
                "хлеб",
                None,
                now_ms,
            )
            .await;
            let (_, payload) =
                ambient_call(&bridge, resolve_command("prop-1", false, true, "idem-1")).await;
            assert_eq!(payload["applied"], true);
            assert_eq!(payload["state"], "muted");
            assert_eq!(payload["task_id"], "", "заглушённое задач не создаёт");
            assert!(bridge
                .journal()
                .list_work_items(AMBIENT_PROPOSAL_PROJECT_ID)
                .await
                .expect("задачи читаются")
                .is_empty());
        }
        // Новый процесс: реестр пуст, единственный источник истины — база.
        let journal = EventJournal::open(&database).expect("journal reopens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator)
            .with_ambient_data_dir(directory.path().to_path_buf());
        let subject_key = crate::ambient_proactivity::subject_key("хлеб");
        let mute_key = crate::ambient_proactivity::mute_key(
            evohime_listener_contract::ProposalKind::Reminder,
            &subject_key,
        );
        assert!(
            bridge.proactivity().is_muted(&journal, &mute_key).await,
            "mute обязан пережить рестарт"
        );
        // И он глушит предложение из другой временной корзины — то есть с
        // другим `proposal_key`.
        let later_now_ms = now_ms + 5 * 60 * 60 * 1000;
        let later_key = crate::ambient_proactivity::proposal_key(
            evohime_listener_contract::ProposalKind::Reminder,
            &subject_key,
            later_now_ms,
        );
        let later = crate::ambient::proposal_record(crate::ambient::ProposalRecordInput {
            proposal_id: "prop-2",
            proposal_key: &later_key,
            mute_key: &mute_key,
            kind: evohime_listener_contract::ProposalKind::Reminder,
            subject_key: &subject_key,
            subject: "хлеб",
            title: "Напомнить купить хлеб",
            source_episode_id: None,
            now_ms: later_now_ms,
        });
        assert_eq!(
            journal.record_ambient_proposal(&later).await,
            Ok(evohime_local_storage::ambient_store::ProposalInsert::Muted)
        );
    }

    /// Список карточек — единственный путь для человекочитаемого текста, и он
    /// не показывает просроченное как ждущее ответа.
    #[tokio::test]
    async fn the_proposal_list_carries_the_card_text_and_hides_expired_cards() {
        let (bridge, _directory) = ambient_bridge("ambient-proposal-list");
        let now_ms = crate::task_memory::now_millis();
        seed_proposal(
            &bridge,
            "prop-fresh",
            evohime_listener_contract::ProposalKind::Reminder,
            "хлеб",
            None,
            now_ms,
        )
        .await;
        seed_proposal(
            &bridge,
            "prop-stale",
            evohime_listener_contract::ProposalKind::Suggestion,
            "отчёт",
            None,
            now_ms - 2 * crate::ambient_proactivity::PROPOSAL_LIFETIME_MS,
        )
        .await;
        let (event_type, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::ListAmbientProposals(
                generated::ListAmbientProposals { limit: 50 },
            ),
        )
        .await;
        assert_eq!(event_type, "ambient.proposals");
        let rows = payload["proposals"].as_array().expect("список карточек");
        assert_eq!(rows.len(), 1, "просроченная карточка снята со списка");
        assert_eq!(rows[0]["proposal_id"], "prop-fresh");
        assert_eq!(rows[0]["title"], "Напомнить купить хлеб");
        assert_eq!(payload["max_per_hour"], 3);
        assert_eq!(payload["max_per_day"], 10);
        assert_eq!(payload["min_interval_ms"], 600_000);
    }

    /// Ни при каких входных данных `ambient.proposal` в журнале не несёт ни
    /// текста карточки, ни темы человеческими словами.
    #[tokio::test]
    async fn the_journalled_proposal_event_carries_no_card_text() {
        let (bridge, _directory) = ambient_bridge("ambient-proposal-privacy");
        let now_ms = crate::task_memory::now_millis();
        seed_proposal(
            &bridge,
            "prop-1",
            evohime_listener_contract::ProposalKind::Reminder,
            "секретный пароль от банка",
            None,
            now_ms,
        )
        .await;
        let (_, payload) =
            ambient_call(&bridge, resolve_command("prop-1", false, false, "idem-1")).await;
        assert_eq!(payload["applied"], true);
        assert_eq!(payload["state"], "declined");

        let journal = bridge.journal();
        let database = journal.database().lock().await;
        let events = database.read_events_after(0, 100).expect("журнал читается");
        let proposal_events: Vec<_> = events
            .into_iter()
            .filter(|event| event.event_type == "ambient.proposal")
            .collect();
        assert_eq!(proposal_events.len(), 1);
        for event in proposal_events {
            let body = String::from_utf8(event.payload).expect("payload is JSON");
            assert!(!body.contains("секретный"), "{body} несёт тему словами");
            assert!(
                !body.contains("Напомнить купить хлеб"),
                "{body} несёт текст карточки"
            );
            let value: serde_json::Value = serde_json::from_str(&body).expect("payload parses");
            for key in value.as_object().expect("object").keys() {
                assert!(
                    !matches!(
                        key.as_str(),
                        "title" | "subject" | "canonical_subject" | "text"
                    ),
                    "ambient.proposal раскрывает {key}"
                );
            }
        }
    }

    /// «Забыть последние 5 минут» удаляет то, что попало в окно, и оставляет
    /// то, что в него не попало.
    #[tokio::test]
    async fn forgetting_a_window_removes_only_that_window() {
        let (bridge, _directory) = ambient_bridge("ambient-forget");
        let journal = bridge.journal();
        let now_ms = crate::task_memory::now_millis();
        journal
            .open_ambient_episode(
                "ep-3",
                "whisper-base-q5_1",
                "whisper-base-q5_1",
                evohime_listener_contract::ExtractionState::Disabled,
                now_ms - 60 * 60 * 1000,
            )
            .await
            .expect("episode opens");
        for (sequence, offset_ms) in [(0i64, 60 * 60 * 1000u64), (1, 60 * 1000)] {
            journal
                .insert_ambient_utterance(
                    &crate::ambient::AmbientUtteranceInput {
                        utterance_id: format!("ep-3-{sequence}"),
                        episode_id: "ep-3".into(),
                        sequence,
                        started_at_ms: now_ms - offset_ms,
                        duration_ms: 800,
                        text: format!("фраза {sequence}"),
                        language: "ru".into(),
                        avg_logprob: -0.1,
                        redacted: false,
                    },
                    7,
                    2_000,
                )
                .await
                .expect("utterance stored");
        }

        let (event_type, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::ForgetAmbientWindow(
                generated::ForgetAmbientWindow {
                    window_ms: 5 * 60 * 1000,
                    confirmed: true,
                },
            ),
        )
        .await;
        assert_eq!(event_type, "ambient.forgotten");
        assert_eq!(payload["deleted_count"], 1);
        let left = journal
            .list_ambient_utterances("ep-3", 10)
            .await
            .expect("utterances read");
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].sequence, 0);
    }

    // ------------------------------------------------------------------
    // Workflow orchestration (план 06.3).
    // ------------------------------------------------------------------

    fn workflow_bridge(name: &str) -> (IpcBridge, tempfile::TempDir) {
        ambient_bridge(name)
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
            generated::command_envelope::Command::ListWorkflowEvents(
                generated::ListWorkflowEvents {
                    run_id: run_id.clone(),
                    after_sequence: -1,
                    limit: 100,
                },
            ),
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
            generated::command_envelope::Command::ListWorkflowEvents(
                generated::ListWorkflowEvents {
                    run_id,
                    after_sequence: 0,
                    limit: 100,
                },
            ),
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
}
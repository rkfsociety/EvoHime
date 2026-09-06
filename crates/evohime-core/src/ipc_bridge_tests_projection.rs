use super::*;

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


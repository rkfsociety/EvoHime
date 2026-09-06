use super::*;

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
